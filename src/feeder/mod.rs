//! Feeder d'entrée : recopie en continu les entrées du G27 vers un device vJoy.
//!
//! Un worker dédié ouvre le G27 natif et le device vJoy, puis pousse à haute
//! cadence l'état du volant (axes + boutons) dans vJoy via `UpdateVJD`. Le jeu
//! lit alors le device vJoy (le G27 réel peut être masqué par `HidHide`). La
//! recopie d'entrée ne fournit **pas** le retour de force : c'est l'objet de la
//! Phase 5 (pont FFB).

mod mapping;

pub use mapping::position_depuis_entrees;

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use crate::entree::{ErreurLecture, LecteurG27, entrees_depuis_rapport};
use crate::vjoy::{ErreurVjoy, StatutVjd, Vjoy};

/// Délai d'attente d'un rapport HID dans la boucle (ms) ; court pour la latence.
const DELAI_LECTURE_MS: i32 = 5;

/// Erreurs au démarrage du feeder.
#[derive(Debug, thiserror::Error)]
pub enum ErreurFeeder {
    /// La liaison vJoy a échoué (DLL absente, symbole manquant).
    #[error("vJoy indisponible : {0}")]
    Vjoy(#[from] ErreurVjoy),
    /// Le pilote vJoy n'est pas activé.
    #[error("le pilote vJoy n'est pas activé")]
    VjoyInactif,
    /// Le device vJoy demandé n'est pas libre.
    #[error("le device vJoy n°{0} n'est pas disponible (statut : {1:?})")]
    DeviceIndisponible(u32, StatutVjd),
    /// L'acquisition du device vJoy a échoué.
    #[error("acquisition du device vJoy n°{0} impossible")]
    AcquisitionEchouee(u32),
    /// Échec de lecture du G27 (non détecté, mode compat, ouverture).
    #[error("{0}")]
    Lecture(#[from] ErreurLecture),
    /// Échec d'initialisation HID ou de démarrage du worker.
    #[error("démarrage du feeder impossible : {0}")]
    Demarrage(io::Error),
}

/// Feeder actif : recopie les entrées du G27 vers vJoy jusqu'à son arrêt.
pub struct Feeder {
    arret: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Feeder {
    /// Démarre le feeder vers le device vJoy `id_vjoy` (1–16).
    ///
    /// Bloque jusqu'à la fin de l'initialisation pour remonter les erreurs de
    /// setup (vJoy, device, G27) ; la recopie tourne ensuite en arrière-plan.
    ///
    /// # Errors
    ///
    /// Voir [`ErreurFeeder`] : vJoy indisponible/inactif, device occupé,
    /// acquisition échouée, G27 absent ou non natif, ou worker non démarré.
    pub fn demarrer(id_vjoy: u32) -> Result<Self, ErreurFeeder> {
        let arret = Arc::new(AtomicBool::new(false));
        let arret_worker = Arc::clone(&arret);
        let (tx, rx) = mpsc::channel();

        let worker = thread::Builder::new()
            .name("g27-feeder".to_owned())
            .spawn(move || boucle_feeder(id_vjoy, &arret_worker, &tx))
            .map_err(ErreurFeeder::Demarrage)?;

        match rx.recv() {
            Ok(Ok(())) => Ok(Self {
                arret,
                worker: Some(worker),
            }),
            Ok(Err(erreur)) => {
                let _ = worker.join();
                Err(erreur)
            }
            Err(_) => {
                let _ = worker.join();
                Err(ErreurFeeder::Demarrage(io::Error::other(
                    "le worker du feeder s'est arrêté avant la fin de l'initialisation",
                )))
            }
        }
    }

    /// Arrête le feeder et attend la fin du worker (idempotent).
    pub fn arreter(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Feeder {
    fn drop(&mut self) {
        self.arreter();
    }
}

/// Corps du worker : initialise vJoy + le G27, signale le résultat, puis recopie.
fn boucle_feeder(id: u32, arret: &AtomicBool, tx: &Sender<Result<(), ErreurFeeder>>) {
    let vjoy = match preparer_vjoy(id) {
        Ok(vjoy) => vjoy,
        Err(erreur) => {
            let _ = tx.send(Err(erreur));
            return;
        }
    };
    // Le device vJoy est acquis : la garde RAII le relâche (RelinquishVJD) au Drop,
    // quel que soit le mode de sortie du worker — arrêt, erreur ou panique. Elle est
    // déclarée juste après `vjoy` pour être droppée avant lui (la garde l'emprunte).
    let _device = DeviceVjoyAcquis { vjoy: &vjoy, id };

    // Le `HidApi` doit rester vivant tant que le lecteur (et son device) est
    // utilisé : on le garde dans cette portée jusqu'à la fin de la boucle.
    let api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(erreur) => {
            let _ = tx.send(Err(ErreurFeeder::Demarrage(io::Error::other(
                erreur.to_string(),
            ))));
            return;
        }
    };
    let mut lecteur = match LecteurG27::ouvrir(&api) {
        Ok(lecteur) => lecteur,
        Err(erreur) => {
            let _ = tx.send(Err(ErreurFeeder::Lecture(erreur)));
            return;
        }
    };

    let _ = tx.send(Ok(()));

    while !arret.load(Ordering::Relaxed) {
        match lecteur.lire(DELAI_LECTURE_MS) {
            Ok(true) => {
                let entrees = entrees_depuis_rapport(lecteur.rapport());
                let mut position = position_depuis_entrees(&entrees);
                let _ = vjoy.mettre_a_jour(id, &mut position);
            }
            Ok(false) => {}
            // Volant débranché ou erreur HID : on arrête proprement la recopie.
            Err(_) => break,
        }
    }
    // `_device` (garde) est relâché ici, et sur tout retour ou panique ci-dessus.
}

/// Garde RAII : relâche le device vJoy (`RelinquishVJD`) au `Drop`, symétrique de
/// l'acquisition, quel que soit le mode de sortie du worker.
struct DeviceVjoyAcquis<'v> {
    vjoy: &'v Vjoy,
    id: u32,
}

impl Drop for DeviceVjoyAcquis<'_> {
    fn drop(&mut self) {
        // Réinitialiser puis relâcher : le device repart propre et libre, quel que
        // soit le mode de sortie (le statut « Occupé » résiduel observé venait d'un
        // RelinquishVJD non garanti).
        self.vjoy.reinitialiser(self.id);
        self.vjoy.liberer(self.id);
    }
}

/// Charge vJoy, vérifie sa disponibilité et acquiert le device `id`.
///
/// Si le device est resté en statut « possédé » (process précédent mal terminé),
/// on le réinitialise et le relâche avant de tenter l'acquisition.
fn preparer_vjoy(id: u32) -> Result<Vjoy, ErreurFeeder> {
    let vjoy = Vjoy::charger()?;
    if !vjoy.active() {
        return Err(ErreurFeeder::VjoyInactif);
    }
    match vjoy.statut(id) {
        StatutVjd::Libre => {}
        // Résidu d'un process précédent mal terminé (OWN ou BUSY) : on tente de
        // récupérer le device (reset + relinquish) avant de l'acquérir.
        StatutVjd::Possede | StatutVjd::Occupe => {
            vjoy.reinitialiser(id);
            vjoy.liberer(id);
        }
        autre => return Err(ErreurFeeder::DeviceIndisponible(id, autre)),
    }
    if !vjoy.acquerir(id) {
        return Err(ErreurFeeder::AcquisitionEchouee(id));
    }
    Ok(vjoy)
}
