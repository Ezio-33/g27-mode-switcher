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

use crate::autocenter;
use crate::entree::{ErreurLecture, LecteurG27, entrees_depuis_rapport};
use crate::ffb::{MessageFfb, PiloteForce, RecepteurFfb, commande_stop_forces};
use crate::report::{self, OutputReport};
use crate::vjoy::{ErreurVjoy, StatutVjd, Vjoy};

/// Délai d'attente d'un rapport HID dans la boucle (ms) ; court pour la latence.
const DELAI_LECTURE_MS: i32 = 5;

/// Demande de retour de force greffée au feeder.
pub enum DemandeFfb {
    /// Pas de FFB (recopie d'entrée seule).
    Aucune,
    /// Capture seule : transmet les messages FFB bruts (debug `ffb capturer`).
    Capture(Sender<MessageFfb>),
    /// Pont complet : consomme les messages et **pilote la force** du G27 (Phase 5).
    /// `couper_autocentrage` : si `true`, coupe le ressort firmware pendant le pont ;
    /// sinon on le garde actif (résistance/centrage à l'arrêt, sans risque d'oscillation).
    Pont { couper_autocentrage: bool },
}

/// Pause de la boucle quand l'alimentation est suspendue (axes en pause).
const DELAI_PAUSE_MS: u64 = 15;

/// Cadence maximale d'envoi des commandes d'autocentrage modulé (ms) ≈ 20 Hz : assez
/// pour suivre les changements de vitesse sans saturer le bus HID.
const PERIODE_AUTOCENTRAGE_MS: u64 = 50;
/// Variation minimale d'amplitude d'autocentrage avant de réémettre la commande
/// (anti-bavardage : on ignore les micro-variations).
const SEUIL_AUTOCENTRAGE_MAJ: u16 = 0x0600;

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
    #[error(
        "acquisition du device vJoy n°{0} impossible — il est probablement déjà \
         utilisé par une autre instance (fermez la fenêtre GUI ou toute autre console \
         g27-mode-switcher, puis réessayez)"
    )]
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
    /// Selon `ffb`, un récepteur FFB est greffé sur le **même** device acquis (sur le
    /// thread worker) : en [`DemandeFfb::Capture`] les messages bruts sont transmis sur
    /// le `Sender` ; en [`DemandeFfb::Pont`] ils pilotent la force du G27 (autocentrage
    /// gardé ou coupé selon `couper_autocentrage`, `stop` garanti à l'arrêt).
    ///
    /// # Errors
    ///
    /// Voir [`ErreurFeeder`] : vJoy indisponible/inactif, device occupé, acquisition
    /// échouée, G27 absent ou non natif, ou worker non démarré. En cas d'échec après
    /// acquisition, le worker relâche le device vJoy avant de se terminer.
    pub fn demarrer(id_vjoy: u32, ffb: DemandeFfb) -> Result<Self, ErreurFeeder> {
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

/// Greffe le récepteur FFB selon la demande, sur le device vJoy déjà acquis. Renvoie
/// `(canal interne lu par le pilote en mode Pont, récepteur FFB à garder vivant,
/// drapeau « couper l'autocentrage »)`. En mode Pont le récepteur émet vers le canal
/// interne consommé par le [`PiloteForce`].
fn greffer_recepteur(
    ffb: DemandeFfb,
    vjoy: &'static Vjoy,
    id: u32,
) -> (
    Option<mpsc::Receiver<MessageFfb>>,
    Option<RecepteurFfb>,
    bool,
) {
    match ffb {
        DemandeFfb::Aucune => (None, None, false),
        DemandeFfb::Capture(sender) => {
            tracing::debug!("FFB : capture des messages bruts (device vJoy n°{id})");
            (None, Some(RecepteurFfb::enregistrer(vjoy, sender)), false)
        }
        DemandeFfb::Pont {
            couper_autocentrage,
        } => {
            let (tx_ffb, rx_ffb) = mpsc::channel();
            tracing::debug!("FFB : pont de force actif (device vJoy n°{id})");
            (
                Some(rx_ffb),
                Some(RecepteurFfb::enregistrer(vjoy, tx_ffb)),
                couper_autocentrage,
            )
        }
    }
}

/// Corps du worker : acquiert vJoy + ouvre le G27, signale le résultat, puis recopie
/// tant que l'alimentation est active et qu'aucun arrêt n'est demandé.
fn boucle_feeder(
    id: u32,
    actif: &AtomicBool,
    arret: &AtomicBool,
    tx: &Sender<Result<(), ErreurFeeder>>,
    ffb: DemandeFfb,
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
    let (mut entree_pont, _recepteur, couper_autocentrage) = greffer_recepteur(ffb, vjoy, id);
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
    // ⚠️ `garde_force` déclarée APRÈS `_device` ⇒ droppée AVANT lui : on envoie le
    // `stop` (volant neutre) et on restaure l'autocentrage AVANT le `RelinquishVJD`.
    // Priorité physique absolue (cf. `GardeForceG27::drop`).
    let mut pilote: Option<PiloteForce> = None;
    let garde_force: Option<GardeForceG27> = match entree_pont.take() {
        Some(rx) => match GardeForceG27::activer(&api, couper_autocentrage) {
            Ok(garde) => {
                pilote = Some(PiloteForce::new(rx));
                Some(garde)
            }
            Err(erreur) => {
                let _ = tx.send(Err(erreur));
                return;
            }
        },
        None => None,
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

    let debut = std::time::Instant::now();
    let mut modulation = ModulationAutocentrage::new(couper_autocentrage);
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
        // Rafraîchissement FFB (~100 Hz) : même sans nouveau rapport, on réémet la
        // dernière force pour satisfaire le watchdog du G27 (sinon il la relâche).
        if let (Some(pilote), Some(garde)) = (pilote.as_mut(), garde_force.as_ref()) {
            let instant_ms = u64::try_from(debut.elapsed().as_millis()).unwrap_or(u64::MAX);
            if let Some(commande) = pilote.prochaine_commande(instant_ms) {
                garde.envoyer(&commande);
            }
            modulation.rafraichir(pilote, garde, instant_ms);
        }
    }
    // Drop ici (et sur tout retour/panique ci-dessus) : `lecteur`, puis `garde_force`
    // (stop + autocentrage restauré), puis `_device` (RelinquishVJD), puis `_recepteur`.
}

/// Modulation throttlée de l'autocentrage matériel dans la boucle worker : n'émet la
/// commande de force d'autocentrage que périodiquement et sur variation sensible, pour
/// ne pas saturer le bus HID (la force constante occupe déjà la cadence ~100 Hz).
struct ModulationAutocentrage {
    /// Si vrai, l'autocentrage est coupé : on n'émet jamais de commande de force.
    couper: bool,
    /// Dernière amplitude émise (pour ne réémettre que sur variation sensible).
    derniere: Option<u16>,
    /// Prochain instant (ms) où une émission est autorisée.
    prochain_ms: u64,
}

impl ModulationAutocentrage {
    fn new(couper: bool) -> Self {
        Self {
            couper,
            derniere: None,
            prochain_ms: 0,
        }
    }

    /// Émet, si nécessaire, la commande d'autocentrage déduite du ressort du jeu.
    fn rafraichir(&mut self, pilote: &PiloteForce, garde: &GardeForceG27, instant_ms: u64) {
        if self.couper || instant_ms < self.prochain_ms {
            return;
        }
        let magnitude = pilote.magnitude_autocentre();
        if self
            .derniere
            .is_none_or(|precedent| magnitude.abs_diff(precedent) > SEUIL_AUTOCENTRAGE_MAJ)
        {
            garde.regler_autocentrage(magnitude);
            self.derniere = Some(magnitude);
        }
        self.prochain_ms = instant_ms + PERIODE_AUTOCENTRAGE_MS;
    }
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

/// Garde RAII de la **sortie de force** vers le G27 (mode Pont).
///
/// Détient un handle d'écriture dédié au G27 (distinct de celui du lecteur ; les
/// handles HID concurrents au G27 sont validés depuis la Phase 4). À la construction,
/// elle règle l'autocentrage matériel selon `couper_autocentrage` (coupé si une couche
/// FFB complète prend le relais, sinon **activé** pour garder une résistance à l'arrêt).
/// Au `Drop` — garanti sur tous les chemins (arrêt, erreur, panique) — elle remet
/// d'abord le **volant au neutre** (`stop`) puis (ré)active l'autocentrage matériel.
struct GardeForceG27 {
    device: hidapi::HidDevice,
}

impl GardeForceG27 {
    /// Ouvre le handle d'écriture du G27 natif et règle l'autocentrage matériel.
    ///
    /// `couper_autocentrage` : si `true`, coupe le ressort firmware (cas d'une couche
    /// FFB complète) ; sinon on **active** l'autocentrage pour garder une résistance/
    /// centrage à l'arrêt (la force constante du jeu est nulle quand la voiture est
    /// immobile). Ce ressort est piloté par le firmware (open-loop) : aucune
    /// rétroaction, donc aucun risque d'oscillation.
    fn activer(api: &hidapi::HidApi, couper_autocentrage: bool) -> Result<Self, ErreurFeeder> {
        let info = crate::hid::find_native_g27(api).map_err(|manque| match manque {
            crate::hid::NativeLookup::NotNative => ErreurFeeder::Lecture(ErreurLecture::NotNative),
            crate::hid::NativeLookup::NoG27 => ErreurFeeder::Lecture(ErreurLecture::NoG27),
        })?;
        let device = api
            .open_path(info.path.as_c_str())
            .map_err(|erreur| ErreurFeeder::Lecture(ErreurLecture::Ouverture(erreur)))?;
        if couper_autocentrage {
            if let Err(erreur) =
                report::write_report(&device, &autocenter::disable_autocenter_report())
            {
                tracing::warn!("FFB : désactivation de l'autocentrage impossible : {erreur}");
            }
        } else {
            for commande in autocenter::enable_autocenter_reports() {
                if let Err(erreur) = report::write_report(&device, &commande) {
                    tracing::warn!("FFB : activation de l'autocentrage impossible : {erreur}");
                }
            }
        }
        Ok(Self { device })
    }

    /// Envoie une commande de force au G27 (erreur seulement tracée : on ne casse
    /// jamais la boucle temps réel pour un write raté).
    fn envoyer(&self, commande: &OutputReport) {
        if let Err(erreur) = report::write_report(&self.device, commande) {
            tracing::debug!("FFB : écriture de force impossible : {erreur}");
        }
    }

    /// Règle la force de l'autocentrage matériel (ressort firmware) à `magnitude`
    /// (0..`0xFFFF`). Sert à la modulation vitesse-dépendante du pont (fort à l'arrêt,
    /// doux en roulant). Erreur seulement tracée (jamais bloquante).
    fn regler_autocentrage(&self, magnitude: u16) {
        if let Err(erreur) =
            report::write_report(&self.device, &autocenter::strength_report(magnitude))
        {
            tracing::debug!("FFB : réglage de l'autocentrage impossible : {erreur}");
        }
    }
}

impl Drop for GardeForceG27 {
    fn drop(&mut self) {
        // 1) Priorité physique absolue : volant neutre (stop des forces) AVANT tout.
        let _ = report::write_report(&self.device, &commande_stop_forces());
        // 2) Restaure l'autocentrage matériel coupé au démarrage du pont FFB.
        for commande in autocenter::enable_autocenter_reports() {
            let _ = report::write_report(&self.device, &commande);
        }
    }
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
