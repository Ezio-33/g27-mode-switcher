//! Conversion (pure) des entrées du G27 en position vJoy.

use crate::entree::{CHAPEAU_RELACHE, EntreesG27};
use crate::vjoy::JoystickPositionV2;

/// Valeur maximale des axes vJoy (plage par défaut 0–32767).
const AXE_MAX: i32 = 32767;
/// Valeur de chapeau POV « centré » (relâché) pour vJoy.
const POV_CENTRE: u32 = 0xFFFF_FFFF;
/// Bouton vJoy (1-indexé) assigné à la marche arrière. Les boutons 1–23 sont les vrais
/// boutons HID du G27 (dont les 6 vitesses = boutons 13–18, octet 2 bits 0–5) ; la
/// marche arrière n'a pas de numéro HID propre (bit vendor), on lui réserve le 24.
const NUMERO_BOUTON_MARCHE_ARRIERE: u8 = 24;

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
        buttons: boutons_avec_marche_arriere(entrees).cast_signed(),
        hats: chapeau_vers_pov(entrees.chapeau),
        ..JoystickPositionV2::default()
    }
}

/// Masque des boutons vJoy : les 24 bits HID du G27 + la marche arrière (synthétisée
/// depuis le levier enfoncé) ajoutée sur [`NUMERO_BOUTON_MARCHE_ARRIERE`].
fn boutons_avec_marche_arriere(entrees: &EntreesG27) -> u32 {
    let mut masque = entrees.boutons.masque();
    if entrees.marche_arriere {
        masque |= 1 << (NUMERO_BOUTON_MARCHE_ARRIERE - 1);
    }
    masque
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

/// Convertit le chapeau du G27 (`0–7` = 8 directions, `0` = haut sens horaire ;
/// [`CHAPEAU_RELACHE`] et au-delà = relâché) en POV continu vJoy (centi-degrés :
/// 0, 4500, …, 31500 ; `POV_CENTRE` si relâché).
fn chapeau_vers_pov(chapeau: u8) -> u32 {
    if chapeau >= CHAPEAU_RELACHE {
        POV_CENTRE
    } else {
        u32::from(chapeau) * 4500
    }
}

#[cfg(test)]
mod tests {
    use super::{AXE_MAX, POV_CENTRE, position_depuis_entrees};
    use crate::entree::{EntreesG27, entrees_depuis_rapport};

    fn entrees(volant: u16, accel: u8, frein: u8, embr: u8) -> EntreesG27 {
        let mut rapport = [0u8; 11];
        rapport[0] = 8; // chapeau relâché (nibble bas de l'octet 0)
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
        // Le volant est un axe 14 bits : ses 2 bits de poids faible (boutons 21/22)
        // sont masqués, donc le maximum décodé est 0xFFFC, pas 0xFFFF.
        let position = position_depuis_entrees(&entrees(u16::MAX, u8::MAX, 0, 0));
        assert!(
            (AXE_MAX - 4..=AXE_MAX).contains(&position.axis_x),
            "axe_x={}",
            position.axis_x
        );
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
        let mut rapport = [0u8; 11];
        rapport[0] = 0b0101_0000; // octet 0 bits 4 et 6 = boutons 1 et 3 (nibble bas = chapeau)
        let position = position_depuis_entrees(&entrees_depuis_rapport(&rapport));
        assert_eq!(position.buttons & 0b101, 0b101); // boutons 1 et 3 → bits 0 et 2
    }

    #[test]
    fn marche_arriere_sur_bouton_dedie() {
        use crate::entree::{BIT_LEVIER_ENFONCE, OCTET_LEVIER_ETAT};
        let bit_ma = 1i32 << (super::NUMERO_BOUTON_MARCHE_ARRIERE - 1);
        // Levier au repos : le bouton marche arrière n'est pas armé.
        let mut rapport = [0u8; 11];
        rapport[OCTET_LEVIER_ETAT] = 0x9c;
        assert_eq!(
            position_depuis_entrees(&entrees_depuis_rapport(&rapport)).buttons & bit_ma,
            0
        );
        // Levier enfoncé : la marche arrière arme son bouton dédié.
        rapport[OCTET_LEVIER_ETAT] = 0x9c | BIT_LEVIER_ENFONCE;
        assert_eq!(
            position_depuis_entrees(&entrees_depuis_rapport(&rapport)).buttons & bit_ma,
            bit_ma
        );
    }

    #[test]
    fn chapeau_converti_en_pov() {
        let mut releve = [0u8; 10];
        releve[0] = 8; // chapeau relâché (centré)
        assert_eq!(
            position_depuis_entrees(&entrees_depuis_rapport(&releve)).hats,
            POV_CENTRE
        );
        releve[0] = 0; // direction haut (N) → 0 centi-degré
        assert_eq!(
            position_depuis_entrees(&entrees_depuis_rapport(&releve)).hats,
            0
        );
        releve[0] = 2; // direction est (E) → 9000 centi-degrés
        assert_eq!(
            position_depuis_entrees(&entrees_depuis_rapport(&releve)).hats,
            9000
        );
    }
}
