//! Worker thread possédant le handle HID et exécutant les commandes en série.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::{autocenter, hid, range, switcher};

/// Intervalle de scrutation de l'état du G27 (présence/mode) par le worker.
const STATUS_POLL_INTERVAL: Duration = Duration::from_millis(700);

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
    loop {
        let status = detect_status(&mut api);
        if last_status != Some(status) {
            last_status = Some(status);
            if evt_tx.send(Event::Status(status)).is_err() {
                return; // frontal disparu
            }
        }

        match cmd_rx.recv_timeout(STATUS_POLL_INTERVAL) {
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(command) => {
                handle_command(&mut api, evt_tx, command);
                // Une opération peut changer le mode (bascule) : on force un
                // nouveau diagnostic immédiat au prochain tour de boucle.
                last_status = None;
            }
            Err(RecvTimeoutError::Timeout) => {}
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
        Command::Shutdown => return,
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
