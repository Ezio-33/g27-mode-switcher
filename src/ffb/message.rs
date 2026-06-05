//! Représentation neutre d'un message FFB reçu du jeu (effet ou commande device).
//!
//! Types métier découplés de l'ABI vJoy : le module ne dépend pas du FFI. Les codes
//! bruts (`FFBEType`, `FFBOP`, `FFB_CTRL`) sont traduits en variantes lisibles.

/// Message FFB décodé d'un paquet vJoy.
#[derive(Debug, Clone)]
pub enum MessageFfb {
    /// Création d'un effet dans un bloc (réserve l'index, fixe le type).
    NouvelEffet { bloc: u8, type_effet: TypeEffet },
    /// Paramètres généraux d'un effet (type, durée, gain, direction).
    Rapport {
        bloc: u8,
        type_effet: TypeEffet,
        duree_ms: u16,
        gain: u8,
        direction: u8,
    },
    /// Force constante.
    Constante { bloc: u8, magnitude: i32 },
    /// Effet périodique (carré/sinus/triangle/dent de scie).
    Periodique {
        bloc: u8,
        magnitude: u32,
        offset: i32,
        phase: u32,
        periode: u32,
    },
    /// Effet conditionnel (ressort/amortisseur/inertie/friction).
    Condition {
        bloc: u8,
        centre: i32,
        coeff_pos: i32,
        coeff_neg: i32,
        satur_pos: u32,
        satur_neg: u32,
        deadband: i32,
    },
    /// Force en rampe (du début vers la fin sur la durée de l'effet).
    Rampe { bloc: u8, debut: i32, fin: i32 },
    /// Enveloppe d'attaque/fondu appliquée à un effet.
    Enveloppe {
        bloc: u8,
        niveau_attaque: u32,
        niveau_fondu: u32,
        temps_attaque: u32,
        temps_fondu: u32,
    },
    /// Opération sur un effet (démarrer/solo/arrêter).
    Operation {
        bloc: u8,
        operation: OperationEffet,
        repetitions: u8,
    },
    /// Gain global du device (0–255).
    Gain(u8),
    /// Commande de contrôle device (enable/disable/stop-all/reset/pause/continue).
    Controle(ControleDevice),
}

/// Type d'effet FFB (`FFBEType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeEffet {
    Aucun,
    Constante,
    Rampe,
    Carre,
    Sinus,
    Triangle,
    DentMontante,
    DentDescendante,
    Ressort,
    Amortisseur,
    Inertie,
    Friction,
    Custom,
    /// Code non reconnu.
    Inconnu,
}

impl TypeEffet {
    /// Traduit un code `FFBEType` (0–12).
    #[must_use]
    pub fn depuis_code(code: i32) -> Self {
        match code {
            0 => Self::Aucun,
            1 => Self::Constante,
            2 => Self::Rampe,
            3 => Self::Carre,
            4 => Self::Sinus,
            5 => Self::Triangle,
            6 => Self::DentMontante,
            7 => Self::DentDescendante,
            8 => Self::Ressort,
            9 => Self::Amortisseur,
            10 => Self::Inertie,
            11 => Self::Friction,
            12 => Self::Custom,
            _ => Self::Inconnu,
        }
    }
}

/// Opération sur un effet (`FFBOP`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationEffet {
    Demarrer,
    Solo,
    Arreter,
    /// Code non reconnu.
    Inconnu,
}

impl OperationEffet {
    /// Traduit un code `FFBOP` (1–3).
    #[must_use]
    pub fn depuis_code(code: i32) -> Self {
        match code {
            1 => Self::Demarrer,
            2 => Self::Solo,
            3 => Self::Arreter,
            _ => Self::Inconnu,
        }
    }
}

/// Commande de contrôle device (`FFB_CTRL`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControleDevice {
    Activer,
    Desactiver,
    ArreterTout,
    Reset,
    Pause,
    Continuer,
    /// Code non reconnu.
    Inconnu,
}

impl ControleDevice {
    /// Traduit un code `FFB_CTRL` (1–6).
    #[must_use]
    pub fn depuis_code(code: i32) -> Self {
        match code {
            1 => Self::Activer,
            2 => Self::Desactiver,
            3 => Self::ArreterTout,
            4 => Self::Reset,
            5 => Self::Pause,
            6 => Self::Continuer,
            _ => Self::Inconnu,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ControleDevice, OperationEffet, TypeEffet};

    #[test]
    fn type_effet_mappe_les_codes() {
        assert_eq!(TypeEffet::depuis_code(1), TypeEffet::Constante);
        assert_eq!(TypeEffet::depuis_code(8), TypeEffet::Ressort);
        assert_eq!(TypeEffet::depuis_code(11), TypeEffet::Friction);
        assert_eq!(TypeEffet::depuis_code(99), TypeEffet::Inconnu);
    }

    #[test]
    fn operation_mappe_les_codes() {
        assert_eq!(OperationEffet::depuis_code(1), OperationEffet::Demarrer);
        assert_eq!(OperationEffet::depuis_code(3), OperationEffet::Arreter);
        assert_eq!(OperationEffet::depuis_code(0), OperationEffet::Inconnu);
    }

    #[test]
    fn controle_mappe_les_codes() {
        assert_eq!(ControleDevice::depuis_code(3), ControleDevice::ArreterTout);
        assert_eq!(ControleDevice::depuis_code(4), ControleDevice::Reset);
        assert_eq!(ControleDevice::depuis_code(7), ControleDevice::Inconnu);
    }
}
