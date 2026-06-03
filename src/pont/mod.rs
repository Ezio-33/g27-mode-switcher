//! Pont vJoy : détection des prérequis et orchestration du feeder + masquage.
//!
//! Le « pont » relie le G27 réel à un device vJoy virtuel (recopie des entrées
//! via [`crate::feeder`]) tout en masquant le volant réel au jeu (via
//! [`crate::hidhide`]). Ce module compose ces briques ; il ne contient pas de
//! logique de recopie ni de masquage propre (zéro duplication).
//!
// « HidHide »/« vJoy » sont des noms de produits, pas des identifiants de code.
#![allow(clippy::doc_markdown)]

mod detection;

pub use detection::{Composant, Prerequis, detecter};

use crate::feeder::{self, Feeder};
use crate::hidhide::{self, MasquageGarde};

/// Erreur au démarrage du pont.
#[derive(Debug, thiserror::Error)]
pub enum ErreurPont {
    /// Échec d'initialisation du sous-système HID.
    #[error("initialisation HID impossible : {0}")]
    Hid(String),
    /// Échec du masquage HidHide.
    #[error("masquage du G27 impossible : {0}")]
    Masquage(hidhide::ErreurHidHide),
    /// Échec du démarrage du feeder vJoy.
    #[error("démarrage du feeder impossible : {0}")]
    Feeder(feeder::ErreurFeeder),
}

/// Pont actif : recopie G27 → vJoy + masquage du G27, avec démasquage garanti.
pub struct Pont {
    // ⚠️ ORDRE DE DÉCLARATION INTENTIONNEL : les champs d'une struct sont droppés
    // dans l'ordre de déclaration (haut → bas). `feeder` AVANT `masquage` garantit
    // qu'au `Drop` on arrête d'abord le feeder (stoppe la lecture, libère le device
    // vJoy) PUIS on démasque le G27. Ne pas réordonner sans tenir compte de ça.
    //
    // `feeder` n'est jamais relu : il est conservé uniquement pour son `Drop`.
    #[allow(dead_code)]
    feeder: Feeder,
    masquage: Option<MasquageGarde>,
    id_vjoy: u32,
}

impl Pont {
    /// Démarre le pont vers le device vJoy `id_vjoy`.
    ///
    /// On acquiert le device vJoy (feeder) **avant** de masquer le G27 : ainsi un
    /// échec vJoy ne laisse jamais le volant masqué. Si `masquer`, le G27 est
    /// ensuite caché (notre process restant en liste blanche).
    ///
    /// # Errors
    ///
    /// [`ErreurPont`] selon l'étape qui échoue. Si le masquage échoue, le `feeder`
    /// (local) est relâché — arrêt + RelinquishVJD — avant le retour de l'erreur.
    pub fn demarrer(id_vjoy: u32, masquer: bool) -> Result<Self, ErreurPont> {
        let (feeder, masquage) = assembler(
            masquer,
            || Feeder::demarrer(id_vjoy).map_err(ErreurPont::Feeder),
            || {
                let api =
                    hidapi::HidApi::new().map_err(|erreur| ErreurPont::Hid(erreur.to_string()))?;
                MasquageGarde::activer(&api).map_err(ErreurPont::Masquage)
            },
        )?;
        Ok(Self {
            feeder,
            masquage,
            id_vjoy,
        })
    }

    /// Identifiant du device vJoy alimenté.
    #[must_use]
    pub fn id_vjoy(&self) -> u32 {
        self.id_vjoy
    }

    /// Vrai si le G27 réel est masqué au jeu.
    #[must_use]
    pub fn g27_masque(&self) -> bool {
        self.masquage.is_some()
    }
}

/// Assemble le pont dans l'**ordre sûr** : feeder d'abord, masquage ensuite.
///
/// Garantie : `masquer_g27` n'est appelé qu'**après** le succès de
/// `demarrer_feeder` ; si une étape échoue, le `?` propage l'erreur et les valeurs
/// déjà créées (le feeder) sont relâchées par leur `Drop`. **Aucun chemin ne peut
/// laisser le G27 masqué après un échec** : soit le masquage n'a pas été tenté
/// (feeder en échec), soit `masquer_g27` a échoué de façon atomique (pas de garde
/// produite, donc rien de masqué).
fn assembler<Fe, Ma>(
    masquer: bool,
    demarrer_feeder: impl FnOnce() -> Result<Fe, ErreurPont>,
    masquer_g27: impl FnOnce() -> Result<Ma, ErreurPont>,
) -> Result<(Fe, Option<Ma>), ErreurPont> {
    let feeder = demarrer_feeder()?;
    let masquage = if masquer { Some(masquer_g27()?) } else { None };
    Ok((feeder, masquage))
}

#[cfg(test)]
mod tests_assemblage {
    use super::{ErreurPont, assembler};
    use std::cell::Cell;
    use std::rc::Rc;

    /// Faux feeder qui note son arrêt (Drop) dans un drapeau partagé.
    struct FeederFactice(Rc<Cell<bool>>);
    impl Drop for FeederFactice {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    fn erreur() -> ErreurPont {
        ErreurPont::Hid("simulation".to_owned())
    }

    #[test]
    fn echec_feeder_ne_masque_jamais() {
        let masquage_tente = Rc::new(Cell::new(false));
        let drapeau = Rc::clone(&masquage_tente);
        let resultat: Result<((), Option<()>), _> = assembler(
            true,
            || Err(erreur()),
            move || {
                drapeau.set(true);
                Ok(())
            },
        );
        assert!(resultat.is_err());
        assert!(
            !masquage_tente.get(),
            "le masquage ne doit jamais être tenté si le feeder échoue"
        );
    }

    #[test]
    fn echec_masquage_relache_le_feeder_sans_rester_masque() {
        let feeder_arrete = Rc::new(Cell::new(false));
        let drapeau = Rc::clone(&feeder_arrete);
        // Le feeder démarre (et acquiert vJoy) puis le masquage échoue : le feeder
        // local est relâché, et aucune garde de masquage n'est produite.
        let resultat = assembler(
            true,
            move || Ok(FeederFactice(drapeau)),
            || Err::<(), _>(erreur()),
        );
        assert!(resultat.is_err());
        assert!(
            feeder_arrete.get(),
            "le feeder doit être relâché si le masquage échoue"
        );
    }

    #[test]
    fn sans_masquage_ne_touche_pas_au_g27() {
        let resultat = assembler::<(), ()>(false, || Ok(()), || panic!("ne doit pas être appelé"));
        assert!(matches!(resultat, Ok(((), None))));
    }
}
