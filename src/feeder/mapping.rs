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

/// Boutons vJoy (1-indexés) dupliquant les 4 directions cardinales du chapeau (D-pad).
/// Le chapeau est aussi exposé en POV continu (cf. [`chapeau_vers_pov`]), mais certains
/// jeux (Forza) ne distinguent pas les directions d'un POV : ces boutons permettent
/// alors de binder haut/bas/gauche/droite séparément (une diagonale = deux boutons).
/// ⚠️ Nécessite un device vJoy configuré avec **au moins 28 boutons**.
const NUMERO_BOUTON_CHAPEAU_HAUT: u8 = 25;
const NUMERO_BOUTON_CHAPEAU_DROITE: u8 = 26;
const NUMERO_BOUTON_CHAPEAU_BAS: u8 = 27;
const NUMERO_BOUTON_CHAPEAU_GAUCHE: u8 = 28;

/// Remappage des boutons HID du G27 vers les numéros de boutons vJoy souhaités
/// (préférence utilisateur). Indexé par le numéro de bouton **G27** (1–23) ; la valeur
/// est le numéro de bouton **vJoy** émis. C'est une permutation des boutons 1–22 ; le
/// bouton 23 (indicateur « marche arrière enclenchée ») reste inchangé. Indice 0 inutilisé.
const REMAP_BOUTONS: [u8; 24] = [
    0, // indice 0 inutilisé (boutons 1-indexés)
    16, 14, 15, 13, 8, 7, 2, 1, 10, 11, // G27 1–10
    12, 9, 17, 18, 19, 20, 21, 22, 4, 6, // G27 11–20
    3, 5, 23, // G27 21–23
];

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

/// Masque des boutons vJoy : les boutons HID du G27 **remappés** selon
/// [`REMAP_BOUTONS`], + la marche arrière (depuis le levier enfoncé) sur
/// [`NUMERO_BOUTON_MARCHE_ARRIERE`], + les 4 directions du chapeau en boutons.
fn boutons_avec_marche_arriere(entrees: &EntreesG27) -> u32 {
    let mut masque = remapper_boutons(entrees.boutons.masque());
    if entrees.marche_arriere {
        masque |= 1 << (NUMERO_BOUTON_MARCHE_ARRIERE - 1);
    }
    masque |= boutons_chapeau(entrees.chapeau);
    masque
}

/// Duplique les 4 directions cardinales du chapeau en boutons vJoy. Le D-pad du G27
/// a 8 directions (`0`=haut, sens horaire jusqu'à `7`=haut-gauche, `8`=relâché) : une
/// diagonale arme les **deux** boutons cardinaux adjacents (ex. haut-droite = haut +
/// droite). `0` si le chapeau est relâché.
fn boutons_chapeau(chapeau: u8) -> u32 {
    if chapeau >= CHAPEAU_RELACHE {
        return 0;
    }
    let mut masque = 0u32;
    if matches!(chapeau, 7 | 0 | 1) {
        masque |= 1 << (NUMERO_BOUTON_CHAPEAU_HAUT - 1);
    }
    if matches!(chapeau, 1..=3) {
        masque |= 1 << (NUMERO_BOUTON_CHAPEAU_DROITE - 1);
    }
    if matches!(chapeau, 3..=5) {
        masque |= 1 << (NUMERO_BOUTON_CHAPEAU_BAS - 1);
    }
    if matches!(chapeau, 5..=7) {
        masque |= 1 << (NUMERO_BOUTON_CHAPEAU_GAUCHE - 1);
    }
    masque
}

/// Applique [`REMAP_BOUTONS`] : pour chaque bouton G27 armé dans `masque_g27` (bit
/// `n-1` = bouton G27 `n`), arme le bouton vJoy correspondant. Renvoie le masque vJoy.
fn remapper_boutons(masque_g27: u32) -> u32 {
    let mut sortie = 0u32;
    for (bouton_g27, &bouton_vjoy) in REMAP_BOUTONS.iter().enumerate().skip(1) {
        if masque_g27 & (1u32 << (bouton_g27 - 1)) != 0 {
            sortie |= 1u32 << (bouton_vjoy - 1);
        }
    }
    sortie
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
        rapport[0] = 0b0101_0000; // octet 0 bits 4 et 6 = boutons G27 1 et 3 (nibble bas = chapeau)
        let position = position_depuis_entrees(&entrees_depuis_rapport(&rapport));
        // Remappage : G27 1 → vJoy 16 (bit 15), G27 3 → vJoy 15 (bit 14).
        let attendu = (1i32 << 15) | (1i32 << 14);
        assert_eq!(position.buttons & attendu, attendu);
    }

    #[test]
    fn remappage_des_boutons() {
        // Quelques correspondances de la table : G27 8 → vJoy 1, G27 21 → vJoy 3,
        // G27 1 → vJoy 16, et le bouton 23 reste inchangé.
        assert_eq!(super::remapper_boutons(1 << 7), 1 << 0);
        assert_eq!(super::remapper_boutons(1 << 20), 1 << 2);
        assert_eq!(super::remapper_boutons(1 << 0), 1 << 15);
        assert_eq!(super::remapper_boutons(1 << 22), 1 << 22);
        // Bijection : tous les boutons G27 1–23 armés couvrent exactement vJoy 1–23.
        let tous = (0..23).fold(0u32, |acc, bit| acc | (1u32 << bit));
        assert_eq!(super::remapper_boutons(tous), tous);
    }

    #[test]
    fn chapeau_duplique_en_boutons_cardinaux() {
        let haut = 1u32 << (super::NUMERO_BOUTON_CHAPEAU_HAUT - 1);
        let droite = 1u32 << (super::NUMERO_BOUTON_CHAPEAU_DROITE - 1);
        let bas = 1u32 << (super::NUMERO_BOUTON_CHAPEAU_BAS - 1);
        let gauche = 1u32 << (super::NUMERO_BOUTON_CHAPEAU_GAUCHE - 1);
        // Haut (0) → bouton haut seul ; gauche (6) → bouton gauche seul.
        assert_eq!(super::boutons_chapeau(0), haut);
        assert_eq!(super::boutons_chapeau(6), gauche);
        // Diagonale haut-droite (1) → haut + droite ; bas-gauche (5) → bas + gauche.
        assert_eq!(super::boutons_chapeau(1), haut | droite);
        assert_eq!(super::boutons_chapeau(5), bas | gauche);
        // Relâché (8) → aucun bouton.
        assert_eq!(super::boutons_chapeau(8), 0);
    }

    #[test]
    fn chapeau_alimente_pov_et_boutons() {
        // Le chapeau reste exposé en POV ET dupliqué en boutons (octet 0, nibble bas).
        let mut rapport = [0u8; 11];
        rapport[0] = 0; // direction haut (N)
        let position = position_depuis_entrees(&entrees_depuis_rapport(&rapport));
        assert_eq!(position.hats, 0, "POV haut = 0 centi-degré");
        let haut = 1i32 << (super::NUMERO_BOUTON_CHAPEAU_HAUT - 1);
        assert_eq!(position.buttons & haut, haut, "bouton haut aussi armé");
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
