//! Worker thread possédant le handle HID et exécutant les commandes en série.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::{autocenter, hid, range, switcher};

/// Intervalle de scrutation de l'état du G27 (présence/mode) par le worker.
const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(700);
/// Cadence de réémission de l'autocentrage matériel (watchdog firmware) en mode natif,
/// hors pont FFB : assez court pour que le ressort ne relâche jamais (volant qui mollit).
const AUTOCENTER_REFRESH_INTERVAL: Duration = Duration::from_millis(250);

/// État de présence et de mode du G27, détecté en continu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Aucun G27 détecté.
    Absent,
    /// G27 présent en mode compatibilité (`0xC294`).
    Compatibility,
    /// G27 présent en mode natif (`0xC29B`).
    Native,
}

/// Commande adressée au worker par un frontal (GUI).
#[derive(Debug, Clone, Copy)]
pub enum Command {
    /// Basculer le G27 en mode natif (avec réglages post-bascule).
    Switch {
        /// Régler l'angle après la bascule.
        apply_range: bool,
        /// Angle de rotation à appliquer si `apply_range` (degrés).
        range_degrees: u16,
        /// Désactiver l'autocentrage matériel après la bascule.
        disable_autocenter: bool,
    },
    /// Régler l'angle de rotation (mode natif requis).
    SetRange(u16),
    /// Activer (`true`) ou désactiver (`false`) l'autocentrage matériel.
    SetAutocenter {
        /// `true` = réactiver (non implémenté en v0.3.0), `false` = désactiver.
        enable: bool,
    },
    /// Indique qu'un **pont FFB** (vJoy ou Forza) gère désormais l'autocentrage (`true`) ou
    /// l'a relâché (`false`). Quand un pont est actif, la session **suspend** son
    /// rafraîchissement périodique de l'autocentrage pour ne pas se battre avec la
    /// modulation du pont ; quand il s'arrête, elle le **reprend** (sinon le ressort
    /// relâche faute de réémission — volant qui redevient mou).
    PontFfbActif(bool),
    /// Arrêter proprement le worker.
    Shutdown,
}

/// Nature de l'opération exécutée, renvoyée dans un [`OpReport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// Bascule en mode natif.
    Switch,
    /// Réglage de l'angle à la valeur indiquée (degrés).
    Range(u16),
    /// Désactivation de l'autocentrage matériel.
    DisableAutocenter,
    /// Réactivation de l'autocentrage matériel.
    EnableAutocenter,
}

/// Cause d'échec d'une opération, indépendante de la langue d'affichage.
///
/// Le frontal (GUI) traduit ces variantes en messages destinés à l'utilisateur ;
/// la session reste neutre vis-à-vis de la présentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpError {
    /// Aucun G27 n'est branché.
    NoG27,
    /// Le G27 est en mode compatibilité alors que le mode natif est requis.
    NotNative,
    /// Le G27 est déjà en mode natif.
    AlreadyNative,
    /// Angle hors des bornes acceptées (40–900°).
    OutOfRange(u16),
    /// Opération reconnue mais non encore implémentée (prévue ultérieurement).
    Unsupported,
    /// Échec matériel/HID (ouverture, écriture, sous-système indisponible).
    Hardware,
}

impl OpError {
    fn from_switch(error: &switcher::Error) -> Self {
        match error {
            switcher::Error::NoG27Found => Self::NoG27,
            switcher::Error::AlreadyNative => Self::AlreadyNative,
            switcher::Error::Report(_) => Self::Hardware,
        }
    }

    fn from_range(error: &range::Error) -> Self {
        match error {
            range::Error::OutOfRange(value) => Self::OutOfRange(*value),
            range::Error::NotNative => Self::NotNative,
            range::Error::NoG27Found => Self::NoG27,
            range::Error::Report(_) => Self::Hardware,
        }
    }

    fn from_autocenter(error: &autocenter::Error) -> Self {
        match error {
            autocenter::Error::NotNative => Self::NotNative,
            autocenter::Error::NoG27Found => Self::NoG27,
            autocenter::Error::Report(_) => Self::Hardware,
        }
    }
}

/// Résultat d'une opération exécutée par le worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpReport {
    /// Opération concernée.
    pub kind: OpKind,
    /// Succès, ou cause d'échec.
    pub result: Result<(), OpError>,
}

/// Événement émis par le worker vers le frontal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// L'état (présence/mode) du G27 a changé (émis aussi à la première détection).
    Status(Status),
    /// Une commande a été exécutée.
    Op(OpReport),
}

/// Session matérielle : worker thread + canaux de communication non bloquants.
pub struct DeviceSession {
    cmd_tx: Sender<Command>,
    evt_rx: Receiver<Event>,
    worker: Option<JoinHandle<()>>,
}

impl DeviceSession {
    /// Démarre la session : ouvre le sous-système HID dans un worker dédié et
    /// commence à publier l'état du G27.
    #[must_use]
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let worker = thread::spawn(move || worker_loop(&cmd_rx, &evt_tx));
        Self {
            cmd_tx,
            evt_rx,
            worker: Some(worker),
        }
    }

    /// Envoie une commande au worker (non bloquant). Ignorée si le worker est mort.
    pub fn send(&self, command: Command) {
        let _ = self.cmd_tx.send(command);
    }

    /// Draine les événements disponibles sans bloquer (à appeler à chaque frame).
    #[must_use]
    pub fn drain_events(&self) -> Vec<Event> {
        self.evt_rx.try_iter().collect()
    }
}

impl Drop for DeviceSession {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Boucle du worker : scrute l'état périodiquement et exécute les commandes.
fn worker_loop(cmd_rx: &Receiver<Command>, evt_tx: &Sender<Event>) {
    let mut api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(error) => {
            tracing::error!("sous-système HID indisponible : {error}");
            let _ = evt_tx.send(Event::Op(OpReport {
                kind: OpKind::Switch,
                result: Err(OpError::Hardware),
            }));
            return;
        }
    };

    let mut last_status: Option<Status> = None;
    // Autocentrage matériel **voulu** (carte/switch) ; rafraîchi périodiquement en natif.
    let mut autocenter_actif = true;
    // Un pont FFB (vJoy/Forza) gère l'autocentrage → on suspend notre rafraîchissement.
    let mut pont_ffb_actif = false;
    // Prochain diagnostic du mode (re-énumération HID, cadence lente).
    let mut prochain_statut = Instant::now();
    loop {
        // Diagnostic présence/mode à cadence lente uniquement (la re-énumération est coûteuse).
        if Instant::now() >= prochain_statut {
            prochain_statut = Instant::now() + STATUS_POLL_INTERVAL;
            let status = detect_status(&mut api);
            if last_status != Some(status) {
                last_status = Some(status);
                if evt_tx.send(Event::Status(status)).is_err() {
                    return; // frontal disparu
                }
            }
        }

        // Attente courte = cadence du rafraîchissement d'autocentrage.
        match cmd_rx.recv_timeout(AUTOCENTER_REFRESH_INTERVAL) {
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Command::PontFfbActif(actif)) => pont_ffb_actif = actif,
            Ok(command) => {
                // Mémorise l'autocentrage voulu (pour le rafraîchissement « watchdog »).
                match command {
                    Command::SetAutocenter { enable } => autocenter_actif = enable,
                    Command::Switch {
                        disable_autocenter, ..
                    } => autocenter_actif = !disable_autocenter,
                    _ => {}
                }
                handle_command(&mut api, evt_tx, command);
                // Une opération peut changer le mode (bascule) : diagnostic immédiat.
                prochain_statut = Instant::now();
            }
            // Réémission périodique de l'autocentrage (watchdog) : en natif, voulu actif, et
            // si aucun pont FFB ne le gère déjà. Maintient le ressort entre/après les ponts.
            Err(RecvTimeoutError::Timeout) => {
                if last_status == Some(Status::Native) && autocenter_actif && !pont_ffb_actif {
                    autocenter::refresh_autocenter(&api);
                }
            }
        }
    }
}

/// Détermine l'état courant du G27 après re-énumération HID.
fn detect_status(api: &mut hidapi::HidApi) -> Status {
    if let Err(error) = api.refresh_devices() {
        tracing::debug!("re-énumération HID impossible : {error}");
    }
    if hid::find_g27(api, hid::G27Mode::Native).is_ok() {
        Status::Native
    } else if hid::find_g27(api, hid::G27Mode::Compatibility).is_ok() {
        Status::Compatibility
    } else {
        Status::Absent
    }
}

/// Exécute une commande et publie son résultat.
fn handle_command(api: &mut hidapi::HidApi, evt_tx: &Sender<Event>, command: Command) {
    let report = match command {
        Command::Switch {
            apply_range,
            range_degrees,
            disable_autocenter,
        } => OpReport {
            kind: OpKind::Switch,
            result: switcher::switch_with_api(
                api,
                false,
                apply_range,
                range_degrees,
                disable_autocenter,
            )
            .map(|_| ())
            .map_err(|error| OpError::from_switch(&error)),
        },
        Command::SetRange(degrees) => OpReport {
            kind: OpKind::Range(degrees),
            result: range::set_range_with_api(api, degrees)
                .map(|_| ())
                .map_err(|error| OpError::from_range(&error)),
        },
        Command::SetAutocenter { enable: true } => OpReport {
            kind: OpKind::EnableAutocenter,
            result: autocenter::enable_autocenter_with_api(api)
                .map(|_| ())
                .map_err(|error| OpError::from_autocenter(&error)),
        },
        Command::SetAutocenter { enable: false } => OpReport {
            kind: OpKind::DisableAutocenter,
            result: autocenter::disable_autocenter_with_api(api)
                .map(|_| ())
                .map_err(|error| OpError::from_autocenter(&error)),
        },
        // Gérés directement dans la boucle (pas d'opération HID ni d'événement).
        Command::Shutdown | Command::PontFfbActif(_) => return,
    };
    let _ = evt_tx.send(Event::Op(report));
}

#[cfg(test)]
mod tests {
    use super::OpError;
    use crate::{autocenter, range, switcher};

    #[test]
    fn maps_switch_errors() {
        assert_eq!(
            OpError::from_switch(&switcher::Error::NoG27Found),
            OpError::NoG27
        );
        assert_eq!(
            OpError::from_switch(&switcher::Error::AlreadyNative),
            OpError::AlreadyNative
        );
    }

    #[test]
    fn maps_range_errors() {
        assert_eq!(
            OpError::from_range(&range::Error::OutOfRange(39)),
            OpError::OutOfRange(39)
        );
        assert_eq!(
            OpError::from_range(&range::Error::NotNative),
            OpError::NotNative
        );
        assert_eq!(
            OpError::from_range(&range::Error::NoG27Found),
            OpError::NoG27
        );
    }

    #[test]
    fn maps_autocenter_errors() {
        assert_eq!(
            OpError::from_autocenter(&autocenter::Error::NotNative),
            OpError::NotNative
        );
        assert_eq!(
            OpError::from_autocenter(&autocenter::Error::NoG27Found),
            OpError::NoG27
        );
    }
}
