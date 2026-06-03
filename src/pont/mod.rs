//! Pont vJoy : détection des prérequis et orchestration du feeder + masquage.
//!
//! Le « pont » relie le G27 réel à un device vJoy virtuel (recopie des entrées
//! via [`crate::feeder`]) tout en masquant le volant réel au jeu (via
//! [`crate::hidhide`]). Ce module compose ces briques ; il ne contient pas de
//! logique de recopie ni de masquage propre (zéro duplication).
//!
// « HidHide »/« vJoy » sont des noms de produits, pas des identifiants de code.
#![allow(clippy::doc_markdown)]

mod detection;

pub use detection::{Composant, Prerequis, detecter};
