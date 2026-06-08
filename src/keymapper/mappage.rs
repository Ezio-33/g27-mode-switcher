//! Association bouton → touche, avec le mode d'envoi par bouton.

use super::bouton::Bouton;
use super::touche::Touche;

/// Mode d'envoi d'une touche associée à un bouton.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModeBouton {
    /// Touche pressée tant que le bouton est engagé, relâchée en sortant.
    #[default]
    Maintenu,
    /// Appui bref (presser + relâcher) au moment où le bouton est engagé.
    Impulsion,
}

/// Touche associée à un bouton et son mode d'envoi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affectation {
    /// Touche du clavier à envoyer.
    pub touche: Touche,
    /// Mode d'envoi (maintenu ou impulsion).
    pub mode: ModeBouton,
}

impl Affectation {
    /// Crée une affectation maintenue (mode par défaut).
    #[must_use]
    pub fn maintenue(touche: Touche) -> Self {
        Self {
            touche,
            mode: ModeBouton::Maintenu,
        }
    }
}

/// Table d'association des sept boutons mappables vers des touches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mappage {
    affectations: [Option<Affectation>; 7],
}

impl Mappage {
    /// Mappage vide (aucun bouton associé).
    #[must_use]
    pub fn vide() -> Self {
        Self::default()
    }

    /// Mappage par défaut : rapports 1–6 → touches `1`…`6`, marche arrière → `r`,
    /// tous en mode maintenu.
    #[must_use]
    pub fn defaut() -> Self {
        let defauts = [
            (Bouton::Rapport1, "1"),
            (Bouton::Rapport2, "2"),
            (Bouton::Rapport3, "3"),
            (Bouton::Rapport4, "4"),
            (Bouton::Rapport5, "5"),
            (Bouton::Rapport6, "6"),
            (Bouton::MarcheArriere, "r"),
        ];
        let mut mappage = Self::vide();
        for (bouton, touche) in defauts {
            // Les littéraux sont valides par construction ; en cas d'anomalie, le
            // bouton reste simplement non mappé (pas de panique).
            if let Some(touche) = Touche::analyser(touche) {
                mappage.definir(bouton, Some(Affectation::maintenue(touche)));
            }
        }
        mappage
    }

    /// Affectation associée à un bouton, si elle existe.
    #[must_use]
    pub fn affectation(&self, bouton: Bouton) -> Option<&Affectation> {
        self.affectations[bouton.index_bit() as usize].as_ref()
    }

    /// Définit (ou retire avec `None`) l'affectation d'un bouton.
    pub fn definir(&mut self, bouton: Bouton, affectation: Option<Affectation>) {
        self.affectations[bouton.index_bit() as usize] = affectation;
    }
}

#[cfg(test)]
mod tests {
    use super::{Bouton, Mappage, ModeBouton};

    #[test]
    fn mappage_par_defaut_couvre_tous_les_boutons() {
        let mappage = Mappage::defaut();
        for bouton in Bouton::TOUS {
            assert!(
                mappage.affectation(bouton).is_some(),
                "{bouton:?} non mappé"
            );
        }
        assert_eq!(
            mappage
                .affectation(Bouton::Rapport3)
                .unwrap()
                .touche
                .canonique(),
            "3"
        );
        assert_eq!(
            mappage
                .affectation(Bouton::MarcheArriere)
                .unwrap()
                .touche
                .canonique(),
            "r"
        );
        assert_eq!(
            mappage.affectation(Bouton::Rapport1).unwrap().mode,
            ModeBouton::Maintenu
        );
    }

    #[test]
    fn definir_et_retirer_une_affectation() {
        let mut mappage = Mappage::vide();
        assert!(mappage.affectation(Bouton::Rapport1).is_none());
        let touche = super::Touche::analyser("a").unwrap();
        mappage.definir(
            Bouton::Rapport1,
            Some(super::Affectation::maintenue(touche)),
        );
        assert!(mappage.affectation(Bouton::Rapport1).is_some());
        mappage.definir(Bouton::Rapport1, None);
        assert!(mappage.affectation(Bouton::Rapport1).is_none());
    }
}
