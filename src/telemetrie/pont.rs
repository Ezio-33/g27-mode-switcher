//! Orchestration du mode Forza : écoute UDP « Data Out » + écriture de la force au G27.
//!
//! Un worker dédié lie le socket UDP, ouvre le G27 natif, et boucle : décode chaque
//! paquet de télémétrie, calcule le couple ([`couple_depuis_telemetrie`]) et l'écrit au
//! volant par commande `lg4ff` brute. La force est **rafraîchie à chaque tour** (~125 Hz)
//! pour satisfaire le watchdog du G27, et **remise au neutre** si le flux se tarit
//! (jeu fermé/en pause) ou à l'arrêt du pont (RAII). L'**autocentrage matériel est piloté
//! ici** : activé à force nulle au démarrage puis **modulé par la vitesse** (lourd à
//! l'arrêt façon friction de parking, plus léger en roulant) — il fournit la
//! « consistance » du volant, distincte de la force de virage. Il est **restauré** (plein)
//! à l'arrêt du pont.

use std::io;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::autocenter;
use crate::ffb::{commande_force_constante, commande_stop_forces};
use crate::hid::{self, NativeLookup};
use crate::report;

use super::{
    ReglagesForza, Telemetrie, analyser, autocentre_depuis_vitesse, couple_depuis_telemetrie,
    secousse_depuis_bosse,
};

/// Délai d'attente d'un paquet UDP par tour de boucle (ms) : court → ~125 Hz de
/// rafraîchissement de la force même sans nouveau paquet.
const LECTURE_TIMEOUT_MS: u64 = 8;
/// Au-delà de ce délai sans paquet, on remet la force au neutre (sécurité : jeu fermé).
const PERTE_FLUX_MS: u128 = 500;
/// Taille du tampon de réception (les paquets Data Out font ~324 octets).
const TAILLE_TAMPON: usize = 2048;
/// Cadence de réémission de la force d'autocentrage modulée (ms ≈ 20 Hz) : assez pour
/// suivre la vitesse sans saturer le bus HID (la force constante occupe déjà ~125 Hz).
const PERIODE_AUTOCENTRE_MS: u64 = 50;

/// Erreurs au démarrage du mode Forza.
#[derive(Debug, thiserror::Error)]
pub enum ErreurTelemetrie {
    /// Le port UDP n'a pas pu être écouté (déjà utilisé par une autre appli).
    #[error("écoute du flux télémétrie impossible (port occupé ?) : {0}")]
    Socket(io::Error),
    /// Aucun G27 détecté.
    #[error("aucun Logitech G27 détecté")]
    G27Absent,
    /// Un G27 est présent mais en mode compatibilité.
    #[error("le G27 est en mode compatibilité — basculez d'abord en mode natif")]
    G27NonNatif,
    /// Ouverture du handle d'écriture du G27 impossible.
    #[error("ouverture du G27 impossible : {0}")]
    Ouverture(String),
    /// Initialisation du sous-système HID impossible.
    #[error("initialisation HID impossible : {0}")]
    Hid(String),
    /// Le worker n'a pas pu démarrer.
    #[error("démarrage du mode Forza impossible : {0}")]
    Demarrage(io::Error),
}

/// État partagé worker → interface (atomique, sans verrou) pour l'affichage en direct.
#[derive(Default)]
struct EtatPartage {
    /// Nombre total de paquets de télémétrie décodés.
    paquets: AtomicU64,
    /// Le jeu est-il en gameplay actif (dernier paquet) ?
    course_active: AtomicBool,
    /// Reçoit-on actuellement le flux (faux si tari depuis [`PERTE_FLUX_MS`]) ?
    reception: AtomicBool,
    /// Dernière dérive avant ×1000 (milliradians) pour l'affichage.
    derive_milli: AtomicI32,
    /// Dernier couple appliqué au volant.
    couple: AtomicI32,
    /// Dernière amplitude de secousse (route/bosses) pour l'affichage.
    secousse: AtomicI32,
}

impl EtatPartage {
    /// Met à jour l'état depuis une télémétrie fraîche, le couple et la secousse appliqués.
    fn maj(&self, t: &Telemetrie, couple: i32, secousse: i32) {
        self.paquets.fetch_add(1, Ordering::Relaxed);
        self.course_active.store(t.course_active, Ordering::Relaxed);
        self.reception.store(true, Ordering::Relaxed);
        self.derive_milli
            .store(milli(t.derive_avant), Ordering::Relaxed);
        self.couple.store(couple, Ordering::Relaxed);
        self.secousse.store(secousse, Ordering::Relaxed);
    }

    /// Marque la perte du flux (force remise au neutre).
    fn perte(&self) {
        self.reception.store(false, Ordering::Relaxed);
        self.course_active.store(false, Ordering::Relaxed);
        self.couple.store(0, Ordering::Relaxed);
        self.secousse.store(0, Ordering::Relaxed);
    }

    /// Instantané pour l'interface.
    fn lire(&self) -> StatutTelemetrie {
        StatutTelemetrie {
            paquets: self.paquets.load(Ordering::Relaxed),
            course_active: self.course_active.load(Ordering::Relaxed),
            reception: self.reception.load(Ordering::Relaxed),
            derive_avant: rad_depuis_milli(self.derive_milli.load(Ordering::Relaxed)),
            couple: self.couple.load(Ordering::Relaxed),
            secousse: self.secousse.load(Ordering::Relaxed),
        }
    }
}

/// Instantané de l'état du mode Forza pour l'affichage.
#[derive(Debug, Clone, Copy)]
pub struct StatutTelemetrie {
    /// Nombre de paquets de télémétrie décodés depuis le démarrage.
    pub paquets: u64,
    /// Le jeu est-il en gameplay actif ?
    pub course_active: bool,
    /// Reçoit-on le flux en ce moment ?
    pub reception: bool,
    /// Dernière dérive avant (radians).
    pub derive_avant: f32,
    /// Dernier couple appliqué (−10000..10000).
    pub couple: i32,
    /// Dernière amplitude de secousse (route/bosses, 0..10000).
    pub secousse: i32,
}

/// Pont Forza actif : un worker synthétise le retour de force depuis la télémétrie.
pub struct PontTelemetrie {
    arret: Arc<AtomicBool>,
    reglages: Arc<Mutex<ReglagesForza>>,
    etat: Arc<EtatPartage>,
    port: u16,
    worker: Option<JoinHandle<()>>,
}

impl PontTelemetrie {
    /// Démarre l'écoute du flux Data Out sur `port` et l'application de la force au G27.
    ///
    /// Bloque jusqu'à la fin de l'initialisation (liaison du socket + ouverture du G27)
    /// pour remonter les erreurs ; la synthèse tourne ensuite en arrière-plan.
    ///
    /// # Errors
    ///
    /// [`ErreurTelemetrie`] selon l'étape : port occupé, G27 absent/non natif, ouverture
    /// HID impossible, ou worker non démarré.
    pub fn demarrer(port: u16, reglages: ReglagesForza) -> Result<Self, ErreurTelemetrie> {
        let arret = Arc::new(AtomicBool::new(false));
        let reglages = Arc::new(Mutex::new(reglages));
        let etat = Arc::new(EtatPartage::default());
        let (tx, rx) = mpsc::channel();
        let worker = {
            let arret = Arc::clone(&arret);
            let reglages = Arc::clone(&reglages);
            let etat = Arc::clone(&etat);
            thread::Builder::new()
                .name("g27-telemetrie".to_owned())
                .spawn(move || boucle(port, &arret, &reglages, &etat, &tx))
                .map_err(ErreurTelemetrie::Demarrage)?
        };
        match rx.recv() {
            Ok(Ok(())) => Ok(Self {
                arret,
                reglages,
                etat,
                port,
                worker: Some(worker),
            }),
            Ok(Err(erreur)) => {
                let _ = worker.join();
                Err(erreur)
            }
            Err(_) => {
                let _ = worker.join();
                Err(ErreurTelemetrie::Demarrage(io::Error::other(
                    "le worker télémétrie s'est arrêté avant la fin de l'initialisation",
                )))
            }
        }
    }

    /// Reconfigure **à chaud** le gain et le sens (le worker les relit au paquet suivant).
    pub fn reconfigurer(&self, reglages: ReglagesForza) {
        if let Ok(mut verrou) = self.reglages.lock() {
            *verrou = reglages;
        }
    }

    /// Port UDP écouté.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Instantané de l'état (réception, dérive, couple) pour l'affichage.
    #[must_use]
    pub fn statut(&self) -> StatutTelemetrie {
        self.etat.lire()
    }
}

impl Drop for PontTelemetrie {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // La remise au neutre du volant est garantie par la garde RAII du worker.
    }
}

/// Garde RAII de la sortie de force vers le G27. À la construction, **active
/// l'autocentrage matériel à force nulle** (prêt à être modulé par la vitesse, au lieu du
/// ressort plein constant qui domine tout). Au `Drop` — garanti sur tous les chemins —
/// remet le volant **au neutre** puis **restaure l'autocentrage plein** (état par défaut).
struct GardeForce {
    device: hidapi::HidDevice,
}

impl GardeForce {
    /// Ouvre la garde et active l'autocentrage matériel à force nulle (modulable ensuite).
    fn nouvelle(device: hidapi::HidDevice) -> Self {
        let _ = report::write_report(&device, &autocenter::strength_report(0));
        let _ = report::write_report(&device, &autocenter::activate_report());
        Self { device }
    }

    /// Applique la force constante (couple de virage) au volant.
    fn appliquer_couple(&self, couple: i32) {
        let _ = report::write_report(&self.device, &commande_force_constante(couple));
    }

    /// Règle la force de l'autocentrage matériel (poids modulé par la vitesse).
    fn appliquer_autocentre(&self, magnitude: u16) {
        let _ = report::write_report(&self.device, &autocenter::strength_report(magnitude));
    }
}

impl Drop for GardeForce {
    fn drop(&mut self) {
        let _ = report::write_report(&self.device, &commande_stop_forces());
        for commande in autocenter::enable_autocenter_reports() {
            let _ = report::write_report(&self.device, &commande);
        }
    }
}

/// Corps du worker : initialise (socket + G27), signale le résultat, puis boucle.
fn boucle(
    port: u16,
    arret: &AtomicBool,
    reglages: &Mutex<ReglagesForza>,
    etat: &EtatPartage,
    tx: &Sender<Result<(), ErreurTelemetrie>>,
) {
    let socket = match lier_socket(port) {
        Ok(socket) => socket,
        Err(erreur) => {
            let _ = tx.send(Err(erreur));
            return;
        }
    };
    let api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(erreur) => {
            let _ = tx.send(Err(ErreurTelemetrie::Hid(erreur.to_string())));
            return;
        }
    };
    let device = match ouvrir_g27(&api) {
        Ok(device) => device,
        Err(erreur) => {
            let _ = tx.send(Err(erreur));
            return;
        }
    };
    let _ = tx.send(Ok(()));
    tracing::debug!("Mode Forza : écoute de la télémétrie sur le port UDP {port}");
    let garde = GardeForce::nouvelle(device);
    boucle_reception(&socket, arret, reglages, etat, &garde);
    // `garde` droppée ici (et sur toute panique) → neutre + autocentrage restauré.
}

/// Boucle de réception/écriture : décode les paquets, applique la force constante (couple
/// de virage) à chaque tour, et l'autocentrage modulé par la vitesse à ~20 Hz.
fn boucle_reception(
    socket: &UdpSocket,
    arret: &AtomicBool,
    reglages: &Mutex<ReglagesForza>,
    etat: &EtatPartage,
    garde: &GardeForce,
) {
    let mut tampon = [0u8; TAILLE_TAMPON];
    let mut dernier_paquet = Instant::now();
    let mut couple = 0;
    let mut secousse = 0;
    let mut magnitude = 0u16;
    let mut compteur: u32 = 0;
    // Débattement de suspension avant de la trame précédente : la **variation** entre deux
    // trames est la source physique des secousses (route, bosses, atterrissages).
    let mut suspension_prec: Option<f32> = None;
    let mut prochain_autocentre = Instant::now();
    while !arret.load(Ordering::Relaxed) {
        match socket.recv(&mut tampon) {
            Ok(taille) => {
                if let Some(t) = analyser(&tampon[..taille]) {
                    dernier_paquet = Instant::now();
                    let r = reglages.lock().map(|g| *g).unwrap_or_default();
                    couple = couple_depuis_telemetrie(&t, &r);
                    magnitude = autocentre_depuis_vitesse(&t, &r);
                    secousse = if t.course_active {
                        let bosse = suspension_prec.map_or(0.0, |p| t.suspension_avant - p);
                        secousse_depuis_bosse(bosse, &r)
                    } else {
                        0
                    };
                    suspension_prec = t.course_active.then_some(t.suspension_avant);
                    etat.maj(&t, couple, secousse);
                }
            }
            // Timeout (pas de paquet ce tour) : on relâche tout si le flux est tari.
            Err(_) if dernier_paquet.elapsed().as_millis() > PERTE_FLUX_MS => {
                couple = 0;
                secousse = 0;
                magnitude = 0;
                suspension_prec = None;
                etat.perte();
            }
            Err(_) => {}
        }
        // Force constante + secousse, dont le signe s'inverse toutes les 2 itérations
        // (~30 Hz) : un **tremblement** franc (route, bosses, atterrissages) plutôt qu'un
        // buzz aigu (~125 Hz) que le poids du volant amortit. Réémis à chaque tour (watchdog).
        compteur = compteur.wrapping_add(1);
        let signe = if (compteur / 2).is_multiple_of(2) {
            1
        } else {
            -1
        };
        garde.appliquer_couple(couple + signe * secousse);
        // Autocentrage modulé réémis périodiquement (suit la vitesse, sans saturer le bus).
        if Instant::now() >= prochain_autocentre {
            garde.appliquer_autocentre(magnitude);
            prochain_autocentre = Instant::now() + Duration::from_millis(PERIODE_AUTOCENTRE_MS);
        }
    }
}

/// Lie le socket UDP d'écoute (toutes interfaces) avec un timeout de lecture court.
fn lier_socket(port: u16) -> Result<UdpSocket, ErreurTelemetrie> {
    let socket = UdpSocket::bind(("0.0.0.0", port)).map_err(ErreurTelemetrie::Socket)?;
    socket
        .set_read_timeout(Some(Duration::from_millis(LECTURE_TIMEOUT_MS)))
        .map_err(ErreurTelemetrie::Socket)?;
    Ok(socket)
}

/// Ouvre le handle d'écriture du G27 natif.
fn ouvrir_g27(api: &hidapi::HidApi) -> Result<hidapi::HidDevice, ErreurTelemetrie> {
    let info = hid::find_native_g27(api).map_err(|raison| match raison {
        NativeLookup::NotNative => ErreurTelemetrie::G27NonNatif,
        NativeLookup::NoG27 => ErreurTelemetrie::G27Absent,
    })?;
    api.open_path(info.path.as_c_str())
        .map_err(|erreur| ErreurTelemetrie::Ouverture(erreur.to_string()))
}

/// Convertit une dérive (radians) en milliradians bornés, pour l'état atomique.
fn milli(valeur: f32) -> i32 {
    let borne = (valeur * 1000.0).clamp(-1_000_000.0, 1_000_000.0);
    // `borne` ∈ [−1e6, 1e6] : la conversion ne peut ni tronquer ni déborder.
    #[allow(clippy::cast_possible_truncation)]
    {
        borne as i32
    }
}

/// Reconvertit des milliradians (état atomique) en radians pour l'affichage.
fn rad_depuis_milli(milli: i32) -> f32 {
    // Plage de dérive réaliste (±quelques rad) : aucune perte de précision sensible.
    #[allow(clippy::cast_precision_loss)]
    {
        milli as f32 / 1000.0
    }
}
