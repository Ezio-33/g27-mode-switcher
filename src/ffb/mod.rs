//! Pont retour de force (FFB) — Phase 5.
//!
//! Reçoit les effets que le jeu envoie au device vJoy et les **décode** en
//! [`MessageFfb`] neutres. Le récepteur se greffe sur le device **déjà acquis et
//! alimenté par le feeder** (cf. [`crate::feeder`] / [`crate::pont`]) : un jeu n'envoie
//! du FFB qu'à un volant vJoy actif (axes alimentés). La traduction en commandes de
//! force Logitech pour le G27 viendra aux étapes suivantes (calcul + envoi).
//!
//! « vJoy »/« FFB » sont des noms de produits/techniques, pas des identifiants.
#![allow(clippy::doc_markdown)]

mod analyse;
mod calcul;
mod effets;
mod g27;
mod message;
mod reception;

pub use calcul::{EtatVolant, couple_net};
pub use effets::{BanqueEffets, Effet, ParametresEffet};
pub use g27::{commande_force_constante, commande_stop_forces};
pub use message::{ControleDevice, MessageFfb, OperationEffet, TypeEffet};
pub use reception::RecepteurFfb;
