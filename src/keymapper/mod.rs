//! Keymapper boîte H → clavier : associe les boutons du G27 à des touches.
//!
//! Ce module ne fait **aucune** entrée/sortie : il modélise les boutons
//! ([`Bouton`]), les touches ([`Touche`]), le mappage ([`Mappage`]) et la
//! **détection de fronts** ([`detecter_fronts`]) qui transforme deux états de
//! boutons successifs en événements clavier ([`Evenement`]). La lecture HID des
//! boutons et l'injection clavier (via `enigo`) sont branchées dans des phases
//! ultérieures, ce qui garde cette logique pure et testable sans matériel.
//!
//! Important : le keymapper **n'occulte pas** les boutons HID natifs du volant —
//! la lecture est non exclusive, le jeu continue de les voir. Les touches sont
//! injectées **en plus**.

mod bouton;
mod mappage;
mod touche;

pub use bouton::Bouton;
pub use mappage::{Affectation, Mappage, ModeBouton};
pub use touche::Touche;

/// État pressé/relâché des sept boutons mappables (un bit par bouton).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EtatBoutons(u8);

impl EtatBoutons {
    /// État sans aucun bouton pressé.
    #[must_use]
    pub fn vide() -> Self {
        Self(0)
    }

    /// Renvoie `true` si le bouton est pressé dans cet état.
    #[must_use]
    pub fn contient(self, bouton: Bouton) -> bool {
        self.0 & (1 << bouton.index_bit()) != 0
    }

    /// Marque un bouton comme pressé (`true`) ou relâché (`false`).
    pub fn definir(&mut self, bouton: Bouton, presse: bool) {
        let bit = 1 << bouton.index_bit();
        if presse {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }
}

/// Action clavier à effectuer pour un événement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Presser et maintenir la touche.
    Presser,
    /// Relâcher la touche.
    Relacher,
    /// Appui bref (presser puis relâcher).
    Taper,
}

/// Un événement clavier à injecter (touche + action).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evenement {
    /// Touche concernée.
    pub touche: Touche,
    /// Action à effectuer.
    pub action: Action,
}

/// Transforme la transition `precedent` → `courant` en événements clavier,
/// selon le mappage et le mode de chaque bouton.
///
/// - mode *maintenu* : front montant → [`Action::Presser`], front descendant →
///   [`Action::Relacher`] ;
/// - mode *impulsion* : front montant → [`Action::Taper`], front descendant →
///   aucun événement.
#[must_use]
pub fn detecter_fronts(
    precedent: EtatBoutons,
    courant: EtatBoutons,
    mappage: &Mappage,
) -> Vec<Evenement> {
    let mut evenements = Vec::new();
    for bouton in Bouton::TOUS {
        let Some(affectation) = mappage.affectation(bouton) else {
            continue;
        };
        let avant = precedent.contient(bouton);
        let apres = courant.contient(bouton);
        let action = match (avant, apres, affectation.mode) {
            (false, true, ModeBouton::Maintenu) => Action::Presser,
            (true, false, ModeBouton::Maintenu) => Action::Relacher,
            (false, true, ModeBouton::Impulsion) => Action::Taper,
            _ => continue,
        };
        evenements.push(Evenement {
            touche: affectation.touche.clone(),
            action,
        });
    }
    evenements
}

#[cfg(test)]
mod tests {
    use super::{
        Action, Affectation, Bouton, EtatBoutons, Mappage, ModeBouton, Touche, detecter_fronts,
    };

    fn etat(boutons: &[Bouton]) -> EtatBoutons {
        let mut etat = EtatBoutons::vide();
        for &bouton in boutons {
            etat.definir(bouton, true);
        }
        etat
    }

    #[test]
    fn etat_boutons_pose_et_lit_les_bits() {
        let mut etat = EtatBoutons::vide();
        assert!(!etat.contient(Bouton::Rapport4));
        etat.definir(Bouton::Rapport4, true);
        assert!(etat.contient(Bouton::Rapport4));
        assert!(!etat.contient(Bouton::Rapport5));
        etat.definir(Bouton::Rapport4, false);
        assert!(!etat.contient(Bouton::Rapport4));
    }

    #[test]
    fn mode_maintenu_presse_puis_relache() {
        let mappage = Mappage::defaut();
        // Engager la 3e : front montant → presser '3'.
        let evts = detecter_fronts(etat(&[]), etat(&[Bouton::Rapport3]), &mappage);
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].action, Action::Presser);
        assert_eq!(evts[0].touche.canonique(), "3");
        // Quitter la 3e : front descendant → relâcher '3'.
        let evts = detecter_fronts(etat(&[Bouton::Rapport3]), etat(&[]), &mappage);
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].action, Action::Relacher);
    }

    #[test]
    fn mode_impulsion_tape_seulement_au_front_montant() {
        let mut mappage = Mappage::vide();
        let touche = Touche::analyser("espace").unwrap();
        mappage.definir(
            Bouton::Rapport1,
            Some(Affectation {
                touche,
                mode: ModeBouton::Impulsion,
            }),
        );
        // Front montant → taper.
        let evts = detecter_fronts(etat(&[]), etat(&[Bouton::Rapport1]), &mappage);
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].action, Action::Taper);
        // Front descendant → rien.
        let evts = detecter_fronts(etat(&[Bouton::Rapport1]), etat(&[]), &mappage);
        assert!(evts.is_empty());
    }

    #[test]
    fn bouton_non_mappe_n_emet_rien() {
        let mappage = Mappage::vide();
        let evts = detecter_fronts(etat(&[]), etat(&[Bouton::Rapport2]), &mappage);
        assert!(evts.is_empty());
    }

    #[test]
    fn sans_changement_aucun_evenement() {
        let mappage = Mappage::defaut();
        let courant = etat(&[Bouton::Rapport2]);
        assert!(detecter_fronts(courant, courant, &mappage).is_empty());
    }
}
