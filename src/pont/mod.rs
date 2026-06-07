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

use std::sync::mpsc::Sender;

use crate::feeder::{self, DemandeFfb, Feeder};
use crate::ffb::MessageFfb;
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

/// Pont : recopie G27 → vJoy + masquage du G27, avec démasquage garanti.
///
/// Le device vJoy est acquis **une seule fois** (à la construction) et relâché au
/// `Drop`. [`suspendre`](Pont::suspendre)/[`reprendre`](Pont::reprendre) ne font que
/// basculer l'alimentation des axes et le masquage, **sans** ré-acquérir vJoy : un
/// 2ᵉ `AcquireVJD` dans un process long-vivant (GUI) échouerait (cf. [`crate::feeder`]).
pub struct Pont {
    // ⚠️ ORDRE DE DÉCLARATION INTENTIONNEL : les champs d'une struct sont droppés
    // dans l'ordre de déclaration (haut → bas). `feeder` AVANT `masquage` garantit
    // qu'au `Drop` on arrête d'abord le feeder (stoppe la lecture, libère le device
    // vJoy) PUIS on démasque le G27. Ne pas réordonner sans tenir compte de ça.
    feeder: Feeder,
    masquage: Option<MasquageGarde>,
    /// Préférence de masquage, mémorisée pour [`reprendre`](Pont::reprendre).
    masquer: bool,
    id_vjoy: u32,
}

impl Pont {
    /// Démarre le pont vers le device vJoy `id_vjoy` (acquisition + alimentation).
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
        Self::demarrer_interne(id_vjoy, masquer, DemandeFfb::Aucune)
    }

    /// Comme [`demarrer`](Pont::demarrer), mais greffe un récepteur FFB en **capture** :
    /// chaque paquet FFB reçu est transmis brut sur `ffb` (debug ; le jeu n'envoie du
    /// FFB qu'à un volant vJoy actif, donc alimenté par le feeder).
    ///
    /// # Errors
    ///
    /// Voir [`demarrer`](Pont::demarrer).
    pub fn demarrer_capture_ffb(
        id_vjoy: u32,
        masquer: bool,
        ffb: Sender<MessageFfb>,
    ) -> Result<Self, ErreurPont> {
        Self::demarrer_interne(id_vjoy, masquer, DemandeFfb::Capture(ffb))
    }

    /// Comme [`demarrer`](Pont::demarrer), mais active le **pont FFB complet** : les
    /// effets reçus pilotent la force du G27 (autocentrage coupé pendant le pont,
    /// `stop` garanti à l'arrêt).
    ///
    /// # Errors
    ///
    /// Voir [`demarrer`](Pont::demarrer).
    pub fn demarrer_pont_ffb(id_vjoy: u32, masquer: bool) -> Result<Self, ErreurPont> {
        Self::demarrer_interne(id_vjoy, masquer, DemandeFfb::Pont)
    }

    /// Assemble le pont (feeder + masquage) avec la demande FFB voulue.
    fn demarrer_interne(id_vjoy: u32, masquer: bool, ffb: DemandeFfb) -> Result<Self, ErreurPont> {
        let (feeder, masquage) = assembler(
            masquer,
            move || Feeder::demarrer(id_vjoy, ffb).map_err(ErreurPont::Feeder),
            masquer_g27,
        )?;
        Ok(Self {
            feeder,
            masquage,
            masquer,
            id_vjoy,
        })
    }

    /// Met le pont en pause : coupe l'alimentation des axes et démasque le G27. Le
    /// device vJoy **reste acquis** (relâché seulement au `Drop`).
    pub fn suspendre(&mut self) {
        self.feeder.desactiver();
        self.masquage = None; // démasque le G27 (Drop de la garde).
    }

    /// Reprend le pont après une pause : réactive l'alimentation et re-masque le G27
    /// si nécessaire, **sans** ré-acquérir le device vJoy.
    ///
    /// # Errors
    ///
    /// [`ErreurPont::Hid`]/[`ErreurPont::Masquage`] si le re-masquage échoue (l'axe
    /// est alors alimenté mais le G27 reste visible).
    pub fn reprendre(&mut self) -> Result<(), ErreurPont> {
        self.feeder.activer();
        if self.masquer && self.masquage.is_none() {
            self.masquage = Some(masquer_g27()?);
        }
        Ok(())
    }

    /// Identifiant du device vJoy alimenté.
    #[must_use]
    pub fn id_vjoy(&self) -> u32 {
        self.id_vjoy
    }

    /// Vrai si l'alimentation des axes est active (pont en marche, pas en pause).
    #[must_use]
    pub fn actif(&self) -> bool {
        self.feeder.est_actif()
    }

    /// Vrai si le G27 réel est masqué au jeu.
    #[must_use]
    pub fn g27_masque(&self) -> bool {
        self.masquage.is_some()
    }
}

/// Active le masquage HidHide du G27 (initialise HID puis pose la garde).
fn masquer_g27() -> Result<MasquageGarde, ErreurPont> {
    let api = hidapi::HidApi::new().map_err(|erreur| ErreurPont::Hid(erreur.to_string()))?;
    MasquageGarde::activer(&api).map_err(ErreurPont::Masquage)
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
