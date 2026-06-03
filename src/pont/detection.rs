//! Détection runtime des prérequis du pont (vJoy + HidHide).

use crate::{hidhide, vjoy};

/// État d'un composant prérequis (vJoy ou HidHide).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Composant {
    /// Disponible et utilisable.
    Disponible,
    /// Indisponible, avec la raison (message destiné à l'utilisateur).
    Indisponible(String),
}

impl Composant {
    /// Vrai si le composant est disponible.
    #[must_use]
    pub fn est_disponible(&self) -> bool {
        matches!(self, Self::Disponible)
    }

    /// Raison de l'indisponibilité, le cas échéant.
    #[must_use]
    pub fn raison(&self) -> Option<&str> {
        match self {
            Self::Disponible => None,
            Self::Indisponible(raison) => Some(raison),
        }
    }
}

/// État des prérequis du pont vJoy.
#[derive(Debug, Clone)]
pub struct Prerequis {
    /// État de vJoy (DLL + pilote actif + device configuré).
    pub vjoy: Composant,
    /// État de HidHide (périphérique de contrôle ouvrable).
    pub hidhide: Composant,
}

impl Prerequis {
    /// Vrai si tous les prérequis sont réunis (le pont peut démarrer).
    #[must_use]
    pub fn tout_disponible(&self) -> bool {
        self.vjoy.est_disponible() && self.hidhide.est_disponible()
    }
}

/// Évalue l'état courant de vJoy et de HidHide.
#[must_use]
pub fn detecter() -> Prerequis {
    Prerequis {
        vjoy: detecter_vjoy(),
        hidhide: detecter_hidhide(),
    }
}

/// Détecte vJoy : DLL chargeable, pilote actif, au moins un device configuré.
fn detecter_vjoy() -> Composant {
    match vjoy::Vjoy::charger() {
        Err(erreur) => Composant::Indisponible(erreur.to_string()),
        Ok(vjoy) if !vjoy.active() => Composant::Indisponible(
            "Le pilote vJoy est installé mais inactif. Activez-le (vJoyConf).".to_owned(),
        ),
        Ok(vjoy) if !vjoy.un_device_configure() => Composant::Indisponible(
            "Aucun device vJoy configuré. Créez-en au moins un (#1) dans vJoyConf.".to_owned(),
        ),
        Ok(_) => Composant::Disponible,
    }
}

/// Détecte HidHide : périphérique de contrôle ouvrable.
fn detecter_hidhide() -> Composant {
    if hidhide::disponible() {
        Composant::Disponible
    } else {
        Composant::Indisponible(hidhide::AIDE_HIDHIDE.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{Composant, Prerequis};

    #[test]
    fn tout_disponible_exige_les_deux() {
        let ok = Prerequis {
            vjoy: Composant::Disponible,
            hidhide: Composant::Disponible,
        };
        assert!(ok.tout_disponible());

        let partiel = Prerequis {
            vjoy: Composant::Disponible,
            hidhide: Composant::Indisponible("absent".to_owned()),
        };
        assert!(!partiel.tout_disponible());
    }

    #[test]
    fn raison_exposee_seulement_si_indisponible() {
        assert_eq!(Composant::Disponible.raison(), None);
        assert_eq!(Composant::Indisponible("x".to_owned()).raison(), Some("x"));
    }
}
