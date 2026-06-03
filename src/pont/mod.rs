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

use crate::feeder::{self, Feeder};
use crate::hidhide::{self, MasquageGarde};

/// Erreur au démarrage du pont.
#[derive(Debug, thiserror::Error)]
pub enum ErreurPont {
    /// Échec d'initialisation du sous-système HID.
    #[error("initialisation HID impossible : {0}")]
    Hid(String),
    /// Échec du masquage HidHide.
    #[error("masquage du G27 impossible : {0}")]
    Masquage(hidhide::ErreurHidHide),
    /// Échec du démarrage du feeder vJoy.
    #[error("démarrage du feeder impossible : {0}")]
    Feeder(feeder::ErreurFeeder),
}

/// Pont actif : recopie G27 → vJoy + masquage du G27, avec démasquage garanti.
pub struct Pont {
    // ⚠️ ORDRE DE DÉCLARATION INTENTIONNEL : les champs d'une struct sont droppés
    // dans l'ordre de déclaration (haut → bas). `feeder` AVANT `masquage` garantit
    // qu'au `Drop` on arrête d'abord le feeder (stoppe la lecture, libère le device
    // vJoy) PUIS on démasque le G27. Ne pas réordonner sans tenir compte de ça.
    //
    // `feeder` n'est jamais relu : il est conservé uniquement pour son `Drop`.
    #[allow(dead_code)]
    feeder: Feeder,
    masquage: Option<MasquageGarde>,
    id_vjoy: u32,
}

impl Pont {
    /// Démarre le pont vers le device vJoy `id_vjoy`.
    ///
    /// On acquiert le device vJoy (feeder) **avant** de masquer le G27 : ainsi un
    /// échec vJoy ne laisse jamais le volant masqué. Si `masquer`, le G27 est
    /// ensuite caché (notre process restant en liste blanche).
    ///
    /// # Errors
    ///
    /// [`ErreurPont`] selon l'étape qui échoue. Si le masquage échoue, le `feeder`
    /// (local) est relâché — arrêt + RelinquishVJD — avant le retour de l'erreur.
    pub fn demarrer(id_vjoy: u32, masquer: bool) -> Result<Self, ErreurPont> {
        // 1. Feeder d'abord (acquiert vJoy). En cas d'échec, rien n'a été masqué.
        let feeder = Feeder::demarrer(id_vjoy).map_err(ErreurPont::Feeder)?;
        // 2. Masquage ensuite. Si `MasquageGarde::activer` échoue, `feeder` (local)
        //    est relâché ici → le feeder s'arrête et libère le device vJoy.
        let masquage = if masquer {
            let api =
                hidapi::HidApi::new().map_err(|erreur| ErreurPont::Hid(erreur.to_string()))?;
            Some(MasquageGarde::activer(&api).map_err(ErreurPont::Masquage)?)
        } else {
            None
        };
        Ok(Self {
            feeder,
            masquage,
            id_vjoy,
        })
    }

    /// Identifiant du device vJoy alimenté.
    #[must_use]
    pub fn id_vjoy(&self) -> u32 {
        self.id_vjoy
    }

    /// Vrai si le G27 réel est masqué au jeu.
    #[must_use]
    pub fn g27_masque(&self) -> bool {
        self.masquage.is_some()
    }
}
