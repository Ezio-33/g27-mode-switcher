//! Session matérielle temps réel : pilotage non bloquant du G27.
//!
//! Le module `device` expose une [`session::DeviceSession`] qui possède un
//! **worker thread** détenant un handle [`hidapi::HidApi`] persistant. Les
//! frontaux (la GUI en particulier) lui envoient des commandes et lisent des
//! événements via des canaux, sans jamais bloquer leur boucle de rendu et sans
//! ré-initialiser le sous-système HID à chaque action.

pub mod session;

pub use session::{Command, DeviceSession, Event, OpError, OpKind, OpReport, Status};
