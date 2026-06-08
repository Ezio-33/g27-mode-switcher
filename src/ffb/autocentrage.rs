//! Modulation (pure) de la force de l'autocentrage matériel selon le ressort du jeu.
//!
//! Forza module son effet « ressort » avec la vitesse : fort à l'arrêt (coeff ~2952),
//! doux en roulant (coeff ~900). On recopie cette intensité dans le ressort **firmware**
//! du G27 (autocentrage matériel) : résistance forte à l'arrêt, douce en roulant, jamais
//! nulle (plancher). Ce ressort est piloté par le firmware (open-loop) → aucune
//! rétroaction, donc **aucun risque d'oscillation** (contrairement à un ressort logiciel).

/// Coefficient de ressort (échelle SDK FFB) correspondant à la **pleine** force
/// d'autocentrage. Calé sur la valeur observée à l'arrêt dans Forza (~2952).
const COEFF_PLEINE_FORCE: i32 = 2952;
/// Plancher d'amplitude : on garde toujours un minimum d'autocentrage (« pas tout mou »).
const PLANCHER: u16 = 0x2800;
/// Constante de décroissance du pic (ms) : sans nouvelle valeur, le pic perd ~63 %
/// en ~`TEMPS_DECROISSANCE_MS`. Lisse le clignotement de Forza (qui alterne 2952/0)
/// tout en suivant la baisse progressive du ressort quand la voiture accélère.
const TEMPS_DECROISSANCE_MS: i64 = 500;

/// Suit l'intensité du ressort du jeu et en déduit l'amplitude de l'autocentrage matériel.
#[derive(Debug, Clone, Copy, Default)]
pub struct ModulateurAutocentrage {
    /// Pic lissé du coefficient de ressort (amorti vers 0, rehaussé par la valeur du jeu).
    pic: i32,
}

impl ModulateurAutocentrage {
    /// Intègre le coefficient de ressort courant `coeff` après `ecoule_ms` écoulées :
    /// le pic décroît avec le temps puis est rehaussé à `coeff`. La décroissance lisse
    /// les retombées à 0 transitoires ; le rehaussement suit instantanément une hausse.
    pub fn appliquer(&mut self, coeff: i32, ecoule_ms: u64) {
        let ecoule = i64::try_from(ecoule_ms).unwrap_or(i64::MAX);
        let perte = i64::from(self.pic).saturating_mul(ecoule) / TEMPS_DECROISSANCE_MS;
        let amorti = i32::try_from((i64::from(self.pic) - perte).max(0)).unwrap_or(i32::MAX);
        self.pic = amorti.max(coeff.max(0));
    }

    /// Amplitude d'autocentrage (0..`0xFFFF`) correspondant au pic courant, plancher inclus.
    #[must_use]
    pub fn magnitude(&self) -> u16 {
        let pic = i64::from(self.pic.clamp(0, COEFF_PLEINE_FORCE));
        let etendue = i64::from(u16::MAX - PLANCHER);
        let amplitude = i64::from(PLANCHER) + etendue * pic / i64::from(COEFF_PLEINE_FORCE);
        u16::try_from(amplitude.clamp(0, i64::from(u16::MAX))).unwrap_or(u16::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::{ModulateurAutocentrage, PLANCHER};

    #[test]
    fn arret_donne_la_pleine_force() {
        let mut m = ModulateurAutocentrage::default();
        m.appliquer(2952, 5); // coeff fort (voiture à l'arrêt)
        assert_eq!(m.magnitude(), u16::MAX);
    }

    #[test]
    fn repos_reste_au_plancher() {
        let m = ModulateurAutocentrage::default();
        assert_eq!(m.magnitude(), PLANCHER);
    }

    #[test]
    fn roulage_adoucit_sans_annuler() {
        let mut m = ModulateurAutocentrage::default();
        m.appliquer(948, 5); // coeff de roulage
        let mag = m.magnitude();
        assert!(mag > PLANCHER, "doit dépasser le plancher : {mag}");
        assert!(mag < u16::MAX, "doit être plus doux que l'arrêt : {mag}");
    }

    #[test]
    fn pic_lisse_les_retombees_a_zero() {
        let mut m = ModulateurAutocentrage::default();
        m.appliquer(2952, 5); // fort
        m.appliquer(0, 5); // Forza retombe à 0 (clignotement)
        // Le pic ne s'effondre pas : l'autocentrage reste quasi plein juste après.
        assert!(
            m.magnitude() > u16::MAX / 2,
            "pic effondré : {}",
            m.magnitude()
        );
    }

    #[test]
    fn pic_decroit_sur_la_duree() {
        let mut m = ModulateurAutocentrage::default();
        m.appliquer(2952, 5);
        let fort = m.magnitude();
        // Long silence (coeff 0) : le pic décroît nettement.
        m.appliquer(0, 1000);
        assert!(m.magnitude() < fort, "le pic aurait dû décroître");
    }
}
