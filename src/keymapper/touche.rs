//! Représentation et validation d'une touche du clavier.
//!
//! Une [`Touche`] est stockée sous une forme **canonique** (minuscule, normalisée)
//! qui sert de valeur de configuration. Le vocabulaire accepté est : un caractère
//! imprimable unique (lettre, chiffre, symbole) ou l'un des noms reconnus
//! ci-dessous. La conversion vers `enigo::Key` (injection réelle) est branchée en
//! Phase 3 / Commit 3, et **doit couvrir exactement** ce vocabulaire.

/// Noms de touches spéciales reconnus (en plus des caractères uniques).
pub(crate) const TOUCHES_NOMMEES: &[&str] = &[
    "espace", "entree", "echap", "tab", "haut", "bas", "gauche", "droite", "maj", "ctrl", "alt",
    "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
];

/// Une touche du clavier, sous forme canonique validée.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Touche(String);

impl Touche {
    /// Valide une saisie utilisateur et la normalise en [`Touche`].
    ///
    /// Renvoie `None` si la saisie n'est ni un nom reconnu ni un caractère
    /// imprimable unique.
    #[must_use]
    pub fn analyser(saisie: &str) -> Option<Self> {
        let normalisee = saisie.trim().to_lowercase();
        if normalisee.is_empty() {
            return None;
        }
        if TOUCHES_NOMMEES.contains(&normalisee.as_str()) {
            return Some(Self(normalisee));
        }
        let mut caracteres = normalisee.chars();
        let premier = caracteres.next()?;
        if caracteres.next().is_some() {
            // Plus d'un caractère et pas un nom reconnu.
            return None;
        }
        if premier.is_ascii_graphic() {
            Some(Self(premier.to_string()))
        } else {
            None
        }
    }

    /// Forme canonique (valeur écrite dans la configuration).
    #[must_use]
    pub fn canonique(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Touche;

    #[test]
    fn caractere_unique_normalise_en_minuscule() {
        assert_eq!(Touche::analyser("R").unwrap().canonique(), "r");
        assert_eq!(Touche::analyser("3").unwrap().canonique(), "3");
    }

    #[test]
    fn noms_reconnus_acceptes() {
        assert_eq!(Touche::analyser("F1").unwrap().canonique(), "f1");
        assert_eq!(Touche::analyser(" Espace ").unwrap().canonique(), "espace");
        assert_eq!(Touche::analyser("entree").unwrap().canonique(), "entree");
    }

    #[test]
    fn saisies_invalides_refusees() {
        assert!(Touche::analyser("").is_none());
        assert!(Touche::analyser("   ").is_none());
        assert!(Touche::analyser("ab").is_none()); // plusieurs caractères, pas un nom
        assert!(Touche::analyser("f13").is_none()); // hors de la plage reconnue
    }
}
