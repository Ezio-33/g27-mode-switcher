//! Conversion (pure) des entrées du G27 en position vJoy.

use crate::entree::EntreesG27;
use crate::vjoy::JoystickPositionV2;

/// Valeur maximale des axes vJoy (plage par défaut 0–32767).
const AXE_MAX: i32 = 32767;
/// Valeur de chapeau POV « centré » (relâché) pour vJoy.
const POV_CENTRE: u32 = 0xFFFF_FFFF;

/// Convertit les entrées décodées du G27 en position vJoy.
///
/// Mappage : volant → axe X, accélérateur → axe Y, frein → axe Z, embrayage →
/// curseur ; boutons recopiés tels quels ; chapeau converti en POV continu.
#[must_use]
pub fn position_depuis_entrees(entrees: &EntreesG27) -> JoystickPositionV2 {
    JoystickPositionV2 {
        axis_x: vers_axe(u32::from(entrees.volant), u32::from(u16::MAX)),
        axis_y: vers_axe(u32::from(entrees.accelerateur), u32::from(u8::MAX)),
        axis_z: vers_axe(u32::from(entrees.frein), u32::from(u8::MAX)),
        slider: vers_axe(u32::from(entrees.embrayage), u32::from(u8::MAX)),
        buttons: entrees.boutons.masque().cast_signed(),
        hats: chapeau_vers_pov(entrees.chapeau),
        ..JoystickPositionV2::default()
    }
}

/// Met à l'échelle `valeur` (dans `0..=max_entree`) sur `0..=AXE_MAX`.
fn vers_axe(valeur: u32, max_entree: u32) -> i32 {
    if max_entree == 0 {
        return 0;
    }
    let mis_a_l_echelle =
        u64::from(valeur) * u64::from(AXE_MAX.unsigned_abs()) / u64::from(max_entree);
    i32::try_from(mis_a_l_echelle).unwrap_or(AXE_MAX)
}

/// Convertit le chapeau du G27 (0 = relâché, 1–8 = directions) en POV continu
/// vJoy (centi-degrés : 0, 4500, …, 31500 ; `POV_CENTRE` si relâché).
fn chapeau_vers_pov(chapeau: u8) -> u32 {
    if chapeau == 0 || chapeau > 8 {
        POV_CENTRE
    } else {
        u32::from(chapeau - 1) * 4500
    }
}

#[cfg(test)]
mod tests {
    use super::{AXE_MAX, POV_CENTRE, position_depuis_entrees};
    use crate::entree::{EntreesG27, entrees_depuis_rapport};

    fn entrees(volant: u16, accel: u8, frein: u8, embr: u8) -> EntreesG27 {
        let mut rapport = [0u8; 10];
        let [lo, hi] = volant.to_le_bytes();
        rapport[3] = lo;
        rapport[4] = hi;
        rapport[5] = accel;
        rapport[6] = frein;
        rapport[7] = embr;
        entrees_depuis_rapport(&rapport)
    }

    #[test]
    fn axes_mis_a_l_echelle() {
        let position = position_depuis_entrees(&entrees(u16::MAX, u8::MAX, 0, 0));
        assert_eq!(position.axis_x, AXE_MAX);
        assert_eq!(position.axis_y, AXE_MAX);
        assert_eq!(position.axis_z, 0);

        let centre = position_depuis_entrees(&entrees(0x8000, 0, 0, 0));
        assert!(
            (16_000..=16_600).contains(&centre.axis_x),
            "axe={}",
            centre.axis_x
        );

        let zero = position_depuis_entrees(&entrees(0, 0, 0, 0));
        assert_eq!(zero.axis_x, 0);
    }

    #[test]
    fn boutons_recopies() {
        let mut rapport = [0u8; 10];
        rapport[0] = 0b0000_0101; // boutons 1 et 3
        let position = position_depuis_entrees(&entrees_depuis_rapport(&rapport));
        assert_eq!(position.buttons & 0b101, 0b101);
    }

    #[test]
    fn chapeau_converti_en_pov() {
        let mut releve = [0u8; 10];
        releve[2] = 0; // chapeau relâché
        assert_eq!(
            position_depuis_entrees(&entrees_depuis_rapport(&releve)).hats,
            POV_CENTRE
        );
        releve[2] = 1; // direction 1 → 0 centi-degré
        assert_eq!(
            position_depuis_entrees(&entrees_depuis_rapport(&releve)).hats,
            0
        );
        releve[2] = 3; // direction 3 → 9000 centi-degrés
        assert_eq!(
            position_depuis_entrees(&entrees_depuis_rapport(&releve)).hats,
            9000
        );
    }
}
