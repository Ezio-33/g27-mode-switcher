//! Modèle des entrées complètes du G27 (axes + boutons) et lecteur HID partagé.
//!
//! Ce module possède le lecteur HID du volant ([`LecteurG27`]) et le décodage du
//! rapport en entrées complètes ([`EntreesG27`]) : volant, pédales, chapeau et
//! l'état des 24 boutons. Il est consommé par le `keymapper` (qui n'extrait que
//! ses boutons mappables) et par le feeder vJoy (qui recopie tout).
//!
//! ⚠️ Les offsets du rapport HID du G27 natif ne sont **pas testables sans
//! matériel** : ils sont déclarés ici comme **provisoires** et se valident via la
//! commande de debug `entrees` (affichage des octets bruts et des valeurs
//! décodées). Les tests vérifient la cohérence interne du décodage, pas le
//! format réel du volant.

mod lecteur;

pub use lecteur::{ErreurLecture, LecteurG27};

/// Octet (little-endian, u16) de l'axe du volant. **Provisoire.**
const OCTET_VOLANT: usize = 3;
/// Octet de l'accélérateur (0–255). **Provisoire.**
const OCTET_ACCELERATEUR: usize = 5;
/// Octet du frein (0–255). **Provisoire.**
const OCTET_FREIN: usize = 6;
/// Octet de l'embrayage (0–255). **Provisoire.**
const OCTET_EMBRAYAGE: usize = 7;
/// Octet contenant le chapeau (nibble bas, 0–8). **Provisoire.**
const OCTET_CHAPEAU: usize = 2;
/// Nombre de boutons du G27 (lus sur 3 octets, boutons 1–24).
const NB_BOUTONS: u8 = 24;

/// État des 24 boutons du G27 (bit `n-1` = bouton HID `n`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BoutonsG27(u32);

impl BoutonsG27 {
    /// Indique si le bouton HID 1-indexé `numero` (1–24) est pressé.
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
    /// Chapeau directionnel (0 = relâché, 1–8 = directions).
    pub chapeau: u8,
    /// État des boutons.
    pub boutons: BoutonsG27,
}

/// Décode un rapport HID brut en [`EntreesG27`].
///
/// Les octets situés au-delà du rapport fourni sont lus comme `0`.
#[must_use]
pub fn entrees_depuis_rapport(rapport: &[u8]) -> EntreesG27 {
    EntreesG27 {
        volant: mot_le(rapport, OCTET_VOLANT),
        accelerateur: octet(rapport, OCTET_ACCELERATEUR),
        frein: octet(rapport, OCTET_FREIN),
        embrayage: octet(rapport, OCTET_EMBRAYAGE),
        chapeau: octet(rapport, OCTET_CHAPEAU) & 0x0f,
        boutons: BoutonsG27(
            u32::from(octet(rapport, 0))
                | (u32::from(octet(rapport, 1)) << 8)
                | (u32::from(octet(rapport, 2)) << 16),
        ),
    }
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
        let mut rapport = [0u8; 10];
        rapport[0] = 0b0000_0001; // bouton 1
        rapport[1] = 0b0001_0000; // bouton 13 (1re)
        rapport[3] = 0x34; // volant bas
        rapport[4] = 0x12; // volant haut → 0x1234
        rapport[5] = 200; // accélérateur
        rapport[6] = 50; // frein
        rapport[7] = 10; // embrayage
        let entrees = entrees_depuis_rapport(&rapport);
        assert_eq!(entrees.volant, 0x1234);
        assert_eq!(entrees.accelerateur, 200);
        assert_eq!(entrees.frein, 50);
        assert_eq!(entrees.embrayage, 10);
        assert!(entrees.boutons.est_presse(1));
        assert!(entrees.boutons.est_presse(13));
        assert!(!entrees.boutons.est_presse(2));
    }

    #[test]
    fn rapport_vide_donne_des_zeros() {
        let entrees = entrees_depuis_rapport(&[]);
        assert_eq!(entrees.volant, 0);
        assert_eq!(entrees.accelerateur, 0);
        assert_eq!(entrees.boutons, BoutonsG27::default());
    }

    #[test]
    fn numero_de_bouton_hors_plage() {
        let boutons = BoutonsG27::default();
        assert!(!boutons.est_presse(0));
        assert!(!boutons.est_presse(25));
    }
}
