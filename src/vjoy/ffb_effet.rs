//! Structures FFB (`FFB_EFF_*`) renvoyées par les helpers `Ffb_h_*` de vJoy.
//!
//! ABI **sourcée** du wrapper officiel `vJoyInterfaceWrap/Wrapper.cs` (offsets
//! `[FieldOffset]` explicites). Chaque champ après `bloc` est aligné sur 4 octets :
//! dans le C, ce sont des `LONG`/`DWORD` (le C# en lit parfois seulement les 16 bits
//! de poids faible). On modélise donc en `i32`/`u32` pour que `#[repr(C)]` reproduise
//! **exactement** les offsets — et que le buffer fasse la **taille complète** attendue
//! par la DLL (zéro débordement). Les tests `offset_of!`/`size_of` verrouillent tout.

/// `FFB_EFF_REPORT` : paramètres généraux d'un effet (type, durée, gain, direction).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct RapportEffet {
    pub bloc: u8,
    /// Type d'effet (`FFBEType`).
    pub type_effet: i32,
    /// Durée en ms (`0xFFFF` = infini).
    pub duree: u16,
    pub repetition_intervalle: u16,
    pub periode_echantillon: u16,
    /// Gain de l'effet (0–255).
    pub gain: u8,
    pub bouton_declencheur: u8,
    /// Direction polaire (1) ou cartésienne (0) — `BOOL` 4 octets.
    pub polaire: u32,
    /// Direction polaire (0–255 ↔ 0–360°) / composante X.
    pub direction: u8,
    /// Composante Y de la direction.
    pub direction_y: u8,
}

/// `FFB_EFF_CONSTANT` : force constante (`Ffb_h_Eff_Constant`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct EffetConstante {
    pub bloc: u8,
    /// Magnitude signée (−10000..10000).
    pub magnitude: i32,
}

/// `FFB_EFF_RAMP` : force en rampe (`Ffb_h_Eff_Ramp`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct EffetRampe {
    pub bloc: u8,
    /// Magnitude au début (−10000..10000).
    pub debut: i32,
    /// Magnitude à la fin (−10000..10000).
    pub fin: i32,
}

/// `FFB_EFF_PERIOD` : effet périodique (`Ffb_h_Eff_Period`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct EffetPeriodique {
    pub bloc: u8,
    /// Amplitude (0..10000).
    pub magnitude: u32,
    /// Décalage (−10000..10000).
    pub offset: i32,
    /// Phase.
    pub phase: u32,
    /// Période.
    pub periode: u32,
}

/// `FFB_EFF_COND` : effet conditionnel (ressort/amortisseur/inertie/friction).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct EffetCondition {
    pub bloc: u8,
    /// Axe Y plutôt que X (`BOOL` 4 octets).
    pub is_y: u32,
    /// Décalage du point central (−10000..10000).
    pub centre: i32,
    /// Coefficient positif (−10000..10000).
    pub coeff_pos: i32,
    /// Coefficient négatif (−10000..10000).
    pub coeff_neg: i32,
    /// Saturation positive (0..10000).
    pub satur_pos: u32,
    /// Saturation négative (0..10000).
    pub satur_neg: u32,
    /// Bande morte (0..10000).
    pub deadband: i32,
}

/// `FFB_EFF_ENVLP` : enveloppe (attaque/fondu) d'un effet.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct EffetEnveloppe {
    pub bloc: u8,
    pub niveau_attaque: u32,
    pub niveau_fondu: u32,
    pub temps_attaque: u32,
    pub temps_fondu: u32,
}

/// `FFB_EFF_OP` : opération sur un effet (start/solo/stop).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct OperationFfb {
    pub bloc: u8,
    /// Opération (`FFBOP`).
    pub operation: i32,
    pub repetitions: u8,
}

#[cfg(test)]
mod tests {
    use super::{
        EffetCondition, EffetConstante, EffetEnveloppe, EffetPeriodique, EffetRampe, OperationFfb,
        RapportEffet,
    };
    use std::mem::{offset_of, size_of};

    #[test]
    fn offsets_rapport_effet() {
        assert_eq!(offset_of!(RapportEffet, bloc), 0);
        assert_eq!(offset_of!(RapportEffet, type_effet), 4);
        assert_eq!(offset_of!(RapportEffet, duree), 8);
        assert_eq!(offset_of!(RapportEffet, repetition_intervalle), 10);
        assert_eq!(offset_of!(RapportEffet, periode_echantillon), 12);
        assert_eq!(offset_of!(RapportEffet, gain), 14);
        assert_eq!(offset_of!(RapportEffet, bouton_declencheur), 15);
        assert_eq!(offset_of!(RapportEffet, polaire), 16);
        assert_eq!(offset_of!(RapportEffet, direction), 20);
        assert_eq!(offset_of!(RapportEffet, direction_y), 21);
        assert_eq!(size_of::<RapportEffet>(), 24);
    }

    #[test]
    fn offsets_constante() {
        assert_eq!(offset_of!(EffetConstante, bloc), 0);
        assert_eq!(offset_of!(EffetConstante, magnitude), 4);
        assert_eq!(size_of::<EffetConstante>(), 8);
    }

    #[test]
    fn offsets_rampe() {
        assert_eq!(offset_of!(EffetRampe, bloc), 0);
        assert_eq!(offset_of!(EffetRampe, debut), 4);
        assert_eq!(offset_of!(EffetRampe, fin), 8);
        assert_eq!(size_of::<EffetRampe>(), 12);
    }

    #[test]
    fn offsets_periodique() {
        assert_eq!(offset_of!(EffetPeriodique, bloc), 0);
        assert_eq!(offset_of!(EffetPeriodique, magnitude), 4);
        assert_eq!(offset_of!(EffetPeriodique, offset), 8);
        assert_eq!(offset_of!(EffetPeriodique, phase), 12);
        assert_eq!(offset_of!(EffetPeriodique, periode), 16);
        assert_eq!(size_of::<EffetPeriodique>(), 20);
    }

    #[test]
    fn offsets_condition() {
        assert_eq!(offset_of!(EffetCondition, bloc), 0);
        assert_eq!(offset_of!(EffetCondition, is_y), 4);
        assert_eq!(offset_of!(EffetCondition, centre), 8);
        assert_eq!(offset_of!(EffetCondition, coeff_pos), 12);
        assert_eq!(offset_of!(EffetCondition, coeff_neg), 16);
        assert_eq!(offset_of!(EffetCondition, satur_pos), 20);
        assert_eq!(offset_of!(EffetCondition, satur_neg), 24);
        assert_eq!(offset_of!(EffetCondition, deadband), 28);
        assert_eq!(size_of::<EffetCondition>(), 32);
    }

    #[test]
    fn offsets_enveloppe() {
        assert_eq!(offset_of!(EffetEnveloppe, bloc), 0);
        assert_eq!(offset_of!(EffetEnveloppe, niveau_attaque), 4);
        assert_eq!(offset_of!(EffetEnveloppe, niveau_fondu), 8);
        assert_eq!(offset_of!(EffetEnveloppe, temps_attaque), 12);
        assert_eq!(offset_of!(EffetEnveloppe, temps_fondu), 16);
        assert_eq!(size_of::<EffetEnveloppe>(), 20);
    }

    #[test]
    fn offsets_operation() {
        assert_eq!(offset_of!(OperationFfb, bloc), 0);
        assert_eq!(offset_of!(OperationFfb, operation), 4);
        assert_eq!(offset_of!(OperationFfb, repetitions), 8);
        assert_eq!(size_of::<OperationFfb>(), 12);
    }
}
