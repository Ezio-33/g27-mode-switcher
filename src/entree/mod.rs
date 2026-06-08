//! Modèle des entrées complètes du G27 (axes + boutons) et lecteur HID partagé.
//!
//! Ce module possède le lecteur HID du volant ([`LecteurG27`]) et le décodage du
//! rapport en entrées complètes ([`EntreesG27`]) : volant, pédales, chapeau et
//! l'état des 23 boutons. Il est consommé par le `keymapper` (qui n'extrait que
//! ses boutons mappables) et par le feeder vJoy (qui recopie tout).
//!
//! Les offsets sont **décodés du descripteur de rapport HID** du G27 natif (relevé
//! via la commande `descripteur`), pas devinés : disposition exacte rappelée au-dessus
//! des constantes. La commande de debug `entrees` permet de revérifier sur matériel.

mod lecteur;

pub use lecteur::{ErreurLecture, LecteurG27};

// Tous les offsets ci-dessous sont **décodés du descripteur de rapport HID** du G27
// natif (`descripteur` : 166 octets), pas devinés. Disposition du rapport d'entrée
// (88 bits = 11 octets) : chapeau 4 bits (o0 bits 0–3) ; boutons 1–22 = 22 bits
// contigus à partir du bit 4 (o0 bit 4 … o3 bit 1) ; volant X = 14 bits (o3 bits 2–7
// + o4) ; pédales Z/Rz/Y (o5/o6/o7) ; levier H X/Y vendor (o8/o9) ; bouton 23 (o10
// bit 0) ; 7 bits vendor (o10 bits 1–7, dont le bit « levier enfoncé »).

/// Octet de poids faible de l'axe du volant (X, 14 bits sur o3–o4). Décodé du descripteur.
const OCTET_VOLANT: usize = 3;
/// Masque retirant du volant les 2 bits de poids faible de l'octet 3, qui sont en
/// réalité les **boutons 21 et 22** (et non l'axe). Le volant occupe o3 bits 2–7 + o4.
const MASQUE_VOLANT_SANS_BOUTONS: u16 = 0xFFFC;
/// Octet de l'accélérateur (Z, 0–255). Décodé du descripteur.
const OCTET_ACCELERATEUR: usize = 5;
/// Octet du frein (Rz, 0–255). Décodé du descripteur.
const OCTET_FREIN: usize = 6;
/// Octet de l'embrayage (Y, 0–255). Décodé du descripteur.
const OCTET_EMBRAYAGE: usize = 7;
/// Octet contenant le chapeau directionnel : **nibble bas du 1er octet** (hat 4 bits
/// du descripteur). `0`=haut puis sens horaire jusqu'à `7`=haut-gauche, `8`=relâché.
const OCTET_CHAPEAU: usize = 0;
/// Décalage (en bits, depuis l'octet 0) du champ contigu des boutons 1–22 : il suit
/// le chapeau de 4 bits. Bouton `n` (1–22) ⇒ bit `DECALAGE + n − 1` du rapport.
const DECALAGE_BOUTONS: u32 = 4;
/// Masque des 22 boutons contigus (bouton `n` → bit `n−1` une fois recalé).
const MASQUE_22_BOUTONS: u32 = 0x003F_FFFF;
/// Octet portant le bouton 23 (bit 0) — séparé du champ contigu dans le descripteur.
const OCTET_BOUTON_23: usize = 10;
/// Masque du bouton 23 dans [`OCTET_BOUTON_23`].
const BIT_BOUTON_23: u8 = 0x01;
/// Valeur du nibble chapeau lorsque le D-pad est relâché (centré). Les valeurs
/// `0..CHAPEAU_RELACHE` sont les 8 directions ; `CHAPEAU_RELACHE` (et au-delà)
/// signifie « centré ».
pub const CHAPEAU_RELACHE: u8 = 8;
/// Octet de la position **X** du levier de la boîte en H (axe analogique vendor).
/// Décodé du descripteur (octet vendor). Les 6 vitesses émettent EN PLUS un bit de
/// bouton (13–18) à l'engagement ; cet axe brut est supplémentaire.
pub const OCTET_LEVIER_X: usize = 8;
/// Octet de la position **Y** du levier de la boîte en H (axe analogique vendor).
pub const OCTET_LEVIER_Y: usize = 9;
/// Octet d'état du levier de la boîte en H (octet vendor 10).
pub const OCTET_LEVIER_ETAT: usize = 10;
/// Masque du bit « levier enfoncé » dans [`OCTET_LEVIER_ETAT`] : le levier poussé
/// vers le bas, geste qui engage la marche arrière sur le G27. **Validé matériel**
/// (octet 10 : `0x9c` au repos → `0xdc` enfoncé, soit ce bit). Bit vendor, sans
/// numéro de bouton HID propre — on le synthétise côté vJoy.
pub const BIT_LEVIER_ENFONCE: u8 = 0x40;
/// Nombre de boutons HID du G27 (1–23 d'après le descripteur).
const NB_BOUTONS: u8 = 23;

/// État des boutons du G27 (bit `n-1` = bouton HID `n`, 1–23).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BoutonsG27(u32);

impl BoutonsG27 {
    /// Indique si le bouton HID 1-indexé `numero` (1–23) est pressé.
    #[must_use]
    pub fn est_presse(self, numero: u8) -> bool {
        if numero == 0 || numero > NB_BOUTONS {
            return false;
        }
        self.0 & (1 << (numero - 1)) != 0
    }

    /// Masque brut des boutons (bit `n-1` = bouton `n`).
    #[must_use]
    pub fn masque(self) -> u32 {
        self.0
    }
}

/// Entrées complètes du G27 décodées depuis un rapport HID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntreesG27 {
    /// Axe du volant (0–65535, centre ≈ 32768).
    pub volant: u16,
    /// Accélérateur (0–255).
    pub accelerateur: u8,
    /// Frein (0–255).
    pub frein: u8,
    /// Embrayage (0–255).
    pub embrayage: u8,
    /// Chapeau directionnel : `0–7` = 8 directions (`0` = haut, sens horaire),
    /// `8` = relâché (centré).
    pub chapeau: u8,
    /// État des boutons.
    pub boutons: BoutonsG27,
    /// Marche arrière engagée : le levier de la boîte H est poussé vers le bas.
    /// Contrairement aux 6 vitesses (chacune un bit de bouton émis à l'engagement),
    /// la marche arrière n'émet que ce bit « enfoncé » — d'où un traitement à part.
    pub marche_arriere: bool,
}

/// Décode un rapport HID brut en [`EntreesG27`].
///
/// Les octets situés au-delà du rapport fourni sont lus comme `0`.
#[must_use]
pub fn entrees_depuis_rapport(rapport: &[u8]) -> EntreesG27 {
    EntreesG27 {
        volant: mot_le(rapport, OCTET_VOLANT) & MASQUE_VOLANT_SANS_BOUTONS,
        accelerateur: octet(rapport, OCTET_ACCELERATEUR),
        frein: octet(rapport, OCTET_FREIN),
        embrayage: octet(rapport, OCTET_EMBRAYAGE),
        chapeau: octet(rapport, OCTET_CHAPEAU) & 0x0f,
        boutons: BoutonsG27(boutons_depuis_rapport(rapport)),
        marche_arriere: octet(rapport, OCTET_LEVIER_ETAT) & BIT_LEVIER_ENFONCE != 0,
    }
}

/// Extrait le masque des boutons HID (bit `n-1` = bouton `n`) d'après le descripteur.
///
/// Les boutons 1–22 forment un champ contigu de 22 bits débutant au bit
/// [`DECALAGE_BOUTONS`] (juste après le chapeau de 4 bits) ; on lit donc les octets
/// 0–3 comme un mot, on décale du chapeau, puis on masque. Le bouton 23 est isolé sur
/// [`OCTET_BOUTON_23`] bit 0 et ajouté en bit 22.
fn boutons_depuis_rapport(rapport: &[u8]) -> u32 {
    let mot = u32::from(octet(rapport, 0))
        | (u32::from(octet(rapport, 1)) << 8)
        | (u32::from(octet(rapport, 2)) << 16)
        | (u32::from(octet(rapport, 3)) << 24);
    let mut boutons = (mot >> DECALAGE_BOUTONS) & MASQUE_22_BOUTONS;
    if octet(rapport, OCTET_BOUTON_23) & BIT_BOUTON_23 != 0 {
        boutons |= 1 << 22; // bouton 23
    }
    boutons
}

/// Décompose une valeur de chapeau (`0`=haut, sens horaire jusqu'à `7`=haut-gauche ;
/// `≥8` = relâché) en bitmask des 4 directions cardinales : **1=haut, 2=droite, 4=bas,
/// 8=gauche**. Une diagonale arme les deux cardinaux adjacents ; `0` si relâché.
/// Mutualisé par le mapping vJoy (boutons + stick) et l'injection clavier.
#[must_use]
pub fn cardinaux_chapeau(chapeau: u8) -> u8 {
    if chapeau >= CHAPEAU_RELACHE {
        return 0;
    }
    let mut masque = 0u8;
    if matches!(chapeau, 7 | 0 | 1) {
        masque |= 1; // haut
    }
    if matches!(chapeau, 1..=3) {
        masque |= 2; // droite
    }
    if matches!(chapeau, 3..=5) {
        masque |= 4; // bas
    }
    if matches!(chapeau, 5..=7) {
        masque |= 8; // gauche
    }
    masque
}

/// Lit un octet du rapport (0 si hors limites).
fn octet(rapport: &[u8], index: usize) -> u8 {
    rapport.get(index).copied().unwrap_or(0)
}

/// Lit un `u16` little-endian du rapport (octets manquants comptés à 0).
fn mot_le(rapport: &[u8], index: usize) -> u16 {
    u16::from(octet(rapport, index)) | (u16::from(octet(rapport, index + 1)) << 8)
}

#[cfg(test)]
mod tests {
    use super::{BoutonsG27, entrees_depuis_rapport};

    #[test]
    fn decode_axes_et_boutons() {
        let mut rapport = [0u8; 11];
        rapport[0] = 0b0001_1000; // chapeau=8 (relâché) + bit 4 = bouton 1
        rapport[2] = 0b0000_0001; // octet 2 bit 0 = bouton 13 (1re vitesse)
        rapport[3] = 0x34; // volant bas (bits 0–1 = boutons 21/22, ici nuls)
        rapport[4] = 0x12; // volant haut → 0x1234
        rapport[5] = 200; // accélérateur
        rapport[6] = 50; // frein
        rapport[7] = 10; // embrayage
        rapport[10] = 0x01; // octet 10 bit 0 = bouton 23
        let entrees = entrees_depuis_rapport(&rapport);
        assert_eq!(entrees.volant, 0x1234);
        assert_eq!(entrees.accelerateur, 200);
        assert_eq!(entrees.frein, 50);
        assert_eq!(entrees.embrayage, 10);
        assert_eq!(entrees.chapeau, 8);
        assert!(entrees.boutons.est_presse(1));
        assert!(entrees.boutons.est_presse(13));
        assert!(entrees.boutons.est_presse(23));
        assert!(!entrees.boutons.est_presse(2));
    }

    #[test]
    fn boutons_21_22_sur_octet3_pas_dans_le_volant() {
        // Les boutons 21 et 22 sont les bits 0 et 1 de l'octet 3 (descripteur HID),
        // PAS l'axe volant : c'était la cause des « boutons rouges » invisibles.
        let mut rapport = [0u8; 11];
        rapport[3] = 0x01; // bit 0 = bouton 21
        let entrees = entrees_depuis_rapport(&rapport);
        assert!(entrees.boutons.est_presse(21));
        assert!(!entrees.boutons.est_presse(22));
        assert_eq!(
            entrees.volant, 0,
            "le bit du bouton ne doit pas polluer le volant"
        );
        rapport[3] = 0x02; // bit 1 = bouton 22
        let entrees = entrees_depuis_rapport(&rapport);
        assert!(entrees.boutons.est_presse(22));
        assert!(!entrees.boutons.est_presse(21));
        // Un vrai déplacement du volant (bits ≥2 de l'octet 3) n'arme aucun bouton.
        let mut volant = [0u8; 11];
        volant[3] = 0xFC;
        volant[4] = 0xFF;
        let entrees = entrees_depuis_rapport(&volant);
        assert!(!entrees.boutons.est_presse(21));
        assert!(!entrees.boutons.est_presse(22));
        assert_eq!(entrees.volant, 0xFFFC);
    }

    #[test]
    fn chapeau_sur_octet0_jamais_compte_comme_bouton() {
        // Au repos, le nibble bas de l'octet 0 vaut 8 (chapeau centré) : ce doit
        // être le chapeau, pas un bouton (les boutons 1–22 commencent au bit 4).
        let repos = entrees_depuis_rapport(&[0x08]);
        assert_eq!(repos.chapeau, 8);
        assert_eq!(repos.boutons.masque(), 0);
        // Croix gauche (W = 6) : ne déclenche aucun bouton.
        let gauche = entrees_depuis_rapport(&[0x06]);
        assert_eq!(gauche.chapeau, 6);
        assert_eq!(gauche.boutons.masque(), 0);
        // Le 1er bouton (octet 0 bit 4) reste lu malgré le chapeau dans le même octet.
        let bouton1 = entrees_depuis_rapport(&[0x18]); // nibble bas 8 (relâché) + bit 4
        assert_eq!(bouton1.chapeau, 8);
        assert!(bouton1.boutons.est_presse(1));
    }

    #[test]
    fn marche_arriere_depuis_le_bit_levier_enfonce() {
        // Octet 10 au repos (0x9c) : marche arrière relâchée.
        let mut rapport = [0u8; 11];
        rapport[super::OCTET_LEVIER_ETAT] = 0x9c;
        assert!(!entrees_depuis_rapport(&rapport).marche_arriere);
        // Levier enfoncé (0x9c | 0x40 = 0xdc) : marche arrière engagée.
        rapport[super::OCTET_LEVIER_ETAT] = 0xdc;
        assert!(entrees_depuis_rapport(&rapport).marche_arriere);
    }

    #[test]
    fn rapport_vide_donne_des_zeros() {
        let entrees = entrees_depuis_rapport(&[]);
        assert_eq!(entrees.volant, 0);
        assert_eq!(entrees.accelerateur, 0);
        assert_eq!(entrees.boutons, BoutonsG27::default());
        assert!(!entrees.marche_arriere);
    }

    #[test]
    fn numero_de_bouton_hors_plage() {
        let boutons = BoutonsG27::default();
        assert!(!boutons.est_presse(0));
        assert!(!boutons.est_presse(25));
    }
}
