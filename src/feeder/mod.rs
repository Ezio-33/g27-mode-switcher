//! Feeder d'entrée : recopie en continu les entrées du G27 vers un device vJoy.
//!
//! Un worker dédié ouvre le G27 natif et pousse à haute cadence l'état du volant
//! (axes + boutons) dans vJoy via `UpdateVJD`. Le jeu lit alors le device vJoy (le
//! G27 réel peut être masqué par `HidHide`). La recopie d'entrée ne fournit **pas**
//! le retour de force : c'est l'objet de la Phase 5 (pont FFB).
//!
//! ⚠️ **Tout l'accès à vJoy se fait sur le thread worker** (`AcquireVJD`,
//! `UpdateVJD`, `RelinquishVJD`) : jamais sur le thread appelant (a fortiori le
//! thread GUI). vJoyInterface met en place son canal FFB via du fenêtrage Windows ;
//! appeler `AcquireVJD` depuis un thread qui possède des fenêtres (la GUI) peut le
//! bloquer indéfiniment. Le device est **acquis une seule fois** (le worker est
//! conservé toute la session) et relâché **uniquement** à l'arrêt du worker.
//! L'alimentation des axes se met en pause/reprend via un drapeau, sans ré-acquérir.

mod mapping;

pub use mapping::position_depuis_entrees;

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::entree::{ErreurLecture, LecteurG27, entrees_depuis_rapport};
use crate::ffb::{PaquetFfb, RecepteurFfb};
use crate::vjoy::{ErreurVjoy, StatutVjd, Vjoy};

/// Délai d'attente d'un rapport HID dans la boucle (ms) ; court pour la latence.
const DELAI_LECTURE_MS: i32 = 5;

/// Pause de la boucle quand l'alimentation est suspendue (axes en pause).
const DELAI_PAUSE_MS: u64 = 15;

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

/// Feeder actif : un worker recopie les entrées du G27 vers vJoy tant que
/// l'alimentation est activée ; il détient le device vJoy acquis (relâché à l'arrêt).
pub struct Feeder {
    /// Alimentation des axes : `true` = recopie active, `false` = en pause.
    actif: Arc<AtomicBool>,
    /// Demande d'arrêt définitif du worker (le `Drop` la pose puis attend le join).
    arret: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Feeder {
    /// Démarre le feeder vers le device vJoy `id_vjoy` (1–16).
    ///
    /// Le worker acquiert le device vJoy (**une seule fois**), ouvre le G27 et démarre
    /// la recopie (alimentation active). Bloque jusqu'à la fin de l'initialisation
    /// pour remonter les erreurs de setup ; la recopie tourne ensuite en arrière-plan.
    /// **À appeler hors du thread GUI** (cf. en-tête du module).
    ///
    /// Si `ffb` est `Some`, un récepteur FFB est greffé sur le **même** device acquis
    /// (sur le thread worker) et chaque paquet reçu est transmis sur ce `Sender`.
    ///
    /// # Errors
    ///
    /// Voir [`ErreurFeeder`] : vJoy indisponible/inactif, device occupé, acquisition
    /// échouée, G27 absent ou non natif, ou worker non démarré. En cas d'échec après
    /// acquisition, le worker relâche le device vJoy avant de se terminer.
    pub fn demarrer(id_vjoy: u32, ffb: Option<Sender<PaquetFfb>>) -> Result<Self, ErreurFeeder> {
        let actif = Arc::new(AtomicBool::new(true));
        let arret = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();

        let worker = {
            let actif = Arc::clone(&actif);
            let arret = Arc::clone(&arret);
            thread::Builder::new()
                .name("g27-feeder".to_owned())
                .spawn(move || boucle_feeder(id_vjoy, &actif, &arret, &tx, ffb))
                .map_err(ErreurFeeder::Demarrage)?
        };

        match rx.recv() {
            Ok(Ok(())) => Ok(Self {
                actif,
                arret,
                worker: Some(worker),
            }),
            // Le worker a déjà relâché le device vJoy (garde) avant de se terminer.
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

    /// (Ré)active l'alimentation des axes (sans ré-acquérir le device vJoy).
    pub fn activer(&self) {
        self.actif.store(true, Ordering::Relaxed);
    }

    /// Met l'alimentation des axes en pause (le device vJoy reste acquis).
    pub fn desactiver(&self) {
        self.actif.store(false, Ordering::Relaxed);
    }

    /// Indique si l'alimentation des axes est active.
    #[must_use]
    pub fn est_actif(&self) -> bool {
        self.actif.load(Ordering::Relaxed)
    }
}

impl Drop for Feeder {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // Le `RelinquishVJD` est effectué par le worker lui-même (garde
        // `DeviceVjoyAcquis` dans `boucle_feeder`), sur le thread qui a acquis.
    }
}

/// Corps du worker : acquiert vJoy + ouvre le G27, signale le résultat, puis recopie
/// tant que l'alimentation est active et qu'aucun arrêt n'est demandé.
fn boucle_feeder(
    id: u32,
    actif: &AtomicBool,
    arret: &AtomicBool,
    tx: &Sender<Result<(), ErreurFeeder>>,
    ffb: Option<Sender<PaquetFfb>>,
) {
    // AcquireVJD ici, sur le thread worker (jamais sur le thread appelant/GUI).
    let vjoy = match preparer_vjoy(id) {
        Ok(vjoy) => vjoy,
        Err(erreur) => {
            let _ = tx.send(Err(erreur));
            return;
        }
    };
    // ⚠️ ORDRE DE DÉCLARATION = ORDRE DE DROP INVERSE. `_recepteur` (FFB) déclaré
    // AVANT `_device` ⇒ droppé APRÈS lui : on relâche d'abord le device
    // (`RelinquishVJD`, qui stoppe les callbacks) PUIS on libère le userdata FFB.
    let _recepteur = ffb.map(|sender| {
        let recepteur = RecepteurFfb::enregistrer(vjoy, sender);
        tracing::debug!("FFB : callback générique enregistré sur le device vJoy n°{id}");
        recepteur
    });
    // Garde RAII : relâche le device vJoy (reset + RelinquishVJD) au `Drop`, sur CE
    // thread, quel que soit le mode de sortie du worker (arrêt, erreur, panique).
    let _device = DeviceVjoyAcquis { vjoy, id };

    // Le `HidApi` doit rester vivant tant que le lecteur (et son device) est utilisé.
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
    tracing::debug!("Feeder : alimentation des axes active (device vJoy n°{id})");

    while !arret.load(Ordering::Relaxed) {
        if !actif.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(DELAI_PAUSE_MS));
            continue;
        }
        match lecteur.lire(DELAI_LECTURE_MS) {
            Ok(true) => {
                let entrees = entrees_depuis_rapport(lecteur.rapport());
                let mut position = position_depuis_entrees(&entrees);
                let _ = vjoy.mettre_a_jour(id, &mut position);
            }
            Ok(false) => {}
            // Volant débranché ou erreur HID : on met l'alimentation en pause (le
            // device vJoy reste acquis, relâché seulement au Drop). Reprise via
            // « Démarrer » après reconnexion ; sinon redémarrer l'application.
            Err(_) => actif.store(false, Ordering::Relaxed),
        }
    }
    // `_device` (garde) est relâché ici, et sur tout retour ou panique ci-dessus.
}

/// Garde RAII : relâche le device vJoy (reset + `RelinquishVJD`) au `Drop`, sur le
/// thread qui a acquis, quel que soit le mode de sortie.
struct DeviceVjoyAcquis {
    vjoy: &'static Vjoy,
    id: u32,
}

impl Drop for DeviceVjoyAcquis {
    fn drop(&mut self) {
        relacher(self.vjoy, self.id);
    }
}

/// Réinitialise puis relâche le device vJoy (`ResetVJD` + `RelinquishVJD`).
fn relacher(vjoy: &Vjoy, id: u32) {
    vjoy.reinitialiser(id);
    vjoy.liberer(id);
}

/// Charge vJoy (instance partagée du process), vérifie sa disponibilité et acquiert
/// le device `id`.
///
/// Acquisition robuste : si le premier `AcquireVJD` échoue (device laissé non-FREE
/// par un process précédent), on récupère le device (`ResetVJD` + `RelinquishVJD`)
/// puis on réessaie une fois.
fn preparer_vjoy(id: u32) -> Result<&'static Vjoy, ErreurFeeder> {
    let vjoy = Vjoy::partagee()?;
    if !vjoy.active() {
        return Err(ErreurFeeder::VjoyInactif);
    }
    let statut = vjoy.statut(id);
    if matches!(statut, StatutVjd::Absent | StatutVjd::Inconnu) {
        return Err(ErreurFeeder::DeviceIndisponible(id, statut));
    }
    // Premier essai d'acquisition ; si elle échoue (device laissé non-FREE par un
    // process précédent), on récupère le device (reset + relinquish) puis on
    // réessaie une fois.
    tracing::debug!("vJoy : AcquireVJD du device n°{id} (statut initial {statut:?})\u{2026}");
    if !vjoy.acquerir(id) {
        tracing::debug!(
            "vJoy : acquisition échouée, tentative de récupération (reset + relinquish)"
        );
        relacher(vjoy, id);
        if !vjoy.acquerir(id) {
            tracing::warn!("vJoy : AcquireVJD du device n°{id} a échoué après récupération");
            return Err(ErreurFeeder::AcquisitionEchouee(id));
        }
    }
    tracing::debug!("vJoy : device n°{id} acquis");
    Ok(vjoy)
}
