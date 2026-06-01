//! Boutons du G27 mappables vers le clavier (boîte H).
//!
//! On expose les **rapports de la boîte H** (1 à 6) et la **marche arrière**.
//! Numéros de bouton HID repris du pilote Linux `drivers/hid/hid-lg4ff.c` : les
//! rapports 1–6 correspondent aux boutons 13–18, la marche arrière au bouton 23.

/// Un bouton du G27 que l'on peut associer à une touche du clavier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bouton {
    /// 1er rapport de la boîte H (bouton HID 13).
    Rapport1,
    /// 2e rapport (bouton HID 14).
    Rapport2,
    /// 3e rapport (bouton HID 15).
    Rapport3,
    /// 4e rapport (bouton HID 16).
    Rapport4,
    /// 5e rapport (bouton HID 17).
    Rapport5,
    /// 6e rapport (bouton HID 18).
    Rapport6,
    /// Marche arrière (bouton HID 23).
    MarcheArriere,
}

impl Bouton {
    /// Tous les boutons mappables, dans l'ordre d'affichage.
    pub const TOUS: [Bouton; 7] = [
        Self::Rapport1,
        Self::Rapport2,
        Self::Rapport3,
        Self::Rapport4,
        Self::Rapport5,
        Self::Rapport6,
        Self::MarcheArriere,
    ];

    /// Indice de bit (0–6) du bouton dans un [`crate::keymapper::EtatBoutons`].
    #[must_use]
    pub fn index_bit(self) -> u8 {
        match self {
            Self::Rapport1 => 0,
            Self::Rapport2 => 1,
            Self::Rapport3 => 2,
            Self::Rapport4 => 3,
            Self::Rapport5 => 4,
            Self::Rapport6 => 5,
            Self::MarcheArriere => 6,
        }
    }

    /// Numéro du bouton HID correspondant sur le G27 (réf. `hid-lg4ff.c`).
    #[must_use]
    pub fn numero_g27(self) -> u8 {
        match self {
            Self::Rapport1 => 13,
            Self::Rapport2 => 14,
            Self::Rapport3 => 15,
            Self::Rapport4 => 16,
            Self::Rapport5 => 17,
            Self::Rapport6 => 18,
            Self::MarcheArriere => 23,
        }
    }

    /// Clé de configuration (section `[keymapper]`).
    #[must_use]
    pub fn cle(self) -> &'static str {
        match self {
            Self::Rapport1 => "rapport_1",
            Self::Rapport2 => "rapport_2",
            Self::Rapport3 => "rapport_3",
            Self::Rapport4 => "rapport_4",
            Self::Rapport5 => "rapport_5",
            Self::Rapport6 => "rapport_6",
            Self::MarcheArriere => "marche_arriere",
        }
    }

    /// Libellé court pour l'interface graphique.
    #[must_use]
    pub fn libelle(self) -> &'static str {
        match self {
            Self::Rapport1 => "1re",
            Self::Rapport2 => "2e",
            Self::Rapport3 => "3e",
            Self::Rapport4 => "4e",
            Self::Rapport5 => "5e",
            Self::Rapport6 => "6e",
            Self::MarcheArriere => "MA",
        }
    }
}
