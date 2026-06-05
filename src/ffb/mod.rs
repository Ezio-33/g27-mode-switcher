//! Pont retour de force (FFB) — Phase 5.
//!
//! Reçoit les effets que le jeu envoie au device vJoy, les traduira en commandes de
//! force Logitech pour le G27 réel. Pour l'instant (commit 1) : **réception** des
//! paquets ([`RecepteurFfb`]). Le récepteur se greffe sur le device **déjà acquis et
//! alimenté par le feeder** (cf. [`crate::feeder`] / [`crate::pont`]) : un jeu
//! n'envoie du FFB qu'à un volant vJoy actif (axes alimentés).
//!
//! « vJoy »/« FFB » sont des noms de produits/techniques, pas des identifiants.
#![allow(clippy::doc_markdown)]

mod reception;

pub use reception::{PaquetFfb, RecepteurFfb};
