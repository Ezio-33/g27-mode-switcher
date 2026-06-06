//! Banque d'effets FFB : maintient l'état courant des effets déclarés par le jeu.
//!
//! Module **pur** (aucun FFI) : on y applique les [`MessageFfb`] reçus pour suivre,
//! par bloc d'effet, son type, ses paramètres et s'il est en cours. C'est la source
//! que le **calcul du couple** (étape suivante) lira pour produire la force du G27.
//!
//! ⚠️ Le **gain par effet** n'est PAS appliqué ici (les jeux le laissent souvent à 0
//! et pilotent via la magnitude + le gain global) : la banque ne fait que mémoriser
//! l'état ; la pondération est décidée au calcul, sur données réelles.

use std::collections::HashMap;

use super::message::{ControleDevice, MessageFfb, OperationEffet, TypeEffet};

/// Gain global par défaut (255 = 100 %).
const GAIN_MAX: u8 = 255;

/// Paramètres spécifiques d'un effet selon son type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ParametresEffet {
    /// Pas encore de paramètres reçus.
    #[default]
    Aucun,
    /// Force constante.
    Constante { magnitude: i32 },
    /// Effet périodique (carré/sinus/triangle/dent de scie).
    Periodique {
        magnitude: u32,
        offset: i32,
        phase: u32,
        periode: u32,
    },
    /// Effet conditionnel (ressort/amortisseur/inertie/friction).
    Condition {
        centre: i32,
        coeff_pos: i32,
        coeff_neg: i32,
        satur_pos: u32,
        satur_neg: u32,
        deadband: i32,
    },
    /// Force en rampe.
    Rampe { debut: i32, fin: i32 },
}

/// État d'un effet dans un bloc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effet {
    /// Type d'effet (renseigné à la création / au rapport).
    pub type_effet: TypeEffet,
    /// Gain par effet (0–255) — **non pondéré ici**, cf. en-tête de module.
    pub gain: u8,
    /// Durée en ms (`0xFFFF` = infini).
    pub duree_ms: u16,
    /// Direction (polaire 0–255 ou composante X).
    pub direction: u8,
    /// Effet en cours de lecture (entre `Demarrer` et `Arreter`).
    pub en_cours: bool,
    /// Paramètres spécifiques.
    pub params: ParametresEffet,
}

impl Effet {
    /// Crée un effet d'un type donné, à l'arrêt et sans paramètres.
    fn nouveau(type_effet: TypeEffet) -> Self {
        Self {
            type_effet,
            gain: 0,
            duree_ms: 0,
            direction: 0,
            en_cours: false,
            params: ParametresEffet::Aucun,
        }
    }
}

/// État global des effets FFB d'un device.
#[derive(Debug, Clone)]
pub struct BanqueEffets {
    effets: HashMap<u8, Effet>,
    gain_global: u8,
    actif: bool,
}

impl Default for BanqueEffets {
    fn default() -> Self {
        Self {
            effets: HashMap::new(),
            gain_global: GAIN_MAX,
            actif: true,
        }
    }
}

impl BanqueEffets {
    /// Banque vide : actuateurs actifs, gain à 100 %.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applique un message FFB pour mettre à jour l'état.
    pub fn appliquer(&mut self, message: MessageFfb) {
        match message {
            MessageFfb::NouvelEffet { bloc, type_effet } => {
                self.effets.insert(bloc, Effet::nouveau(type_effet));
            }
            MessageFfb::Rapport {
                bloc,
                type_effet,
                duree_ms,
                gain,
                direction,
            } => {
                let effet = self.effet_mut(bloc, type_effet);
                effet.type_effet = type_effet;
                effet.duree_ms = duree_ms;
                effet.gain = gain;
                effet.direction = direction;
            }
            MessageFfb::Constante { bloc, magnitude } => {
                self.maj_params(bloc, ParametresEffet::Constante { magnitude });
            }
            MessageFfb::Periodique {
                bloc,
                magnitude,
                offset,
                phase,
                periode,
            } => self.maj_params(
                bloc,
                ParametresEffet::Periodique {
                    magnitude,
                    offset,
                    phase,
                    periode,
                },
            ),
            MessageFfb::Condition {
                bloc,
                centre,
                coeff_pos,
                coeff_neg,
                satur_pos,
                satur_neg,
                deadband,
            } => self.maj_params(
                bloc,
                ParametresEffet::Condition {
                    centre,
                    coeff_pos,
                    coeff_neg,
                    satur_pos,
                    satur_neg,
                    deadband,
                },
            ),
            MessageFfb::Rampe { bloc, debut, fin } => {
                self.maj_params(bloc, ParametresEffet::Rampe { debut, fin });
            }
            // Enveloppe non encore exploitée (sera utile au calcul attaque/fondu).
            MessageFfb::Enveloppe { .. } => {}
            MessageFfb::Operation {
                bloc, operation, ..
            } => self.operer(bloc, operation),
            MessageFfb::Gain(gain) => self.gain_global = gain,
            MessageFfb::Controle(controle) => self.controler(controle),
        }
    }

    /// Gain global courant (0–255).
    #[must_use]
    pub fn gain_global(&self) -> u8 {
        self.gain_global
    }

    /// Vrai si les actuateurs sont actifs (ni désactivés ni en pause).
    #[must_use]
    pub fn actif(&self) -> bool {
        self.actif
    }

    /// Itère sur les effets actuellement en cours (ordre non garanti).
    pub fn effets_en_cours(&self) -> impl Iterator<Item = &Effet> {
        self.effets.values().filter(|effet| effet.en_cours)
    }

    /// Accède à l'effet d'un bloc (le crée si absent), en fixant son type.
    fn effet_mut(&mut self, bloc: u8, type_effet: TypeEffet) -> &mut Effet {
        self.effets
            .entry(bloc)
            .or_insert_with(|| Effet::nouveau(type_effet))
    }

    /// Met à jour les paramètres d'un effet (le crée si absent, type inconnu).
    fn maj_params(&mut self, bloc: u8, params: ParametresEffet) {
        self.effet_mut(bloc, TypeEffet::Aucun).params = params;
    }

    /// Applique une opération (`Demarrer`/`Solo`/`Arreter`) à un effet.
    fn operer(&mut self, bloc: u8, operation: OperationEffet) {
        match operation {
            OperationEffet::Solo => self.arreter_tout(),
            OperationEffet::Demarrer | OperationEffet::Arreter | OperationEffet::Inconnu => {}
        }
        if let Some(effet) = self.effets.get_mut(&bloc) {
            match operation {
                OperationEffet::Demarrer | OperationEffet::Solo => effet.en_cours = true,
                OperationEffet::Arreter => effet.en_cours = false,
                OperationEffet::Inconnu => {}
            }
        }
    }

    /// Applique une commande de contrôle device.
    fn controler(&mut self, controle: ControleDevice) {
        match controle {
            ControleDevice::Activer | ControleDevice::Continuer => self.actif = true,
            ControleDevice::Desactiver | ControleDevice::Pause => self.actif = false,
            ControleDevice::ArreterTout => self.arreter_tout(),
            ControleDevice::Reset => {
                self.effets.clear();
                self.actif = true;
            }
            ControleDevice::Inconnu => {}
        }
    }

    /// Arrête tous les effets sans les supprimer.
    fn arreter_tout(&mut self) {
        for effet in self.effets.values_mut() {
            effet.en_cours = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BanqueEffets, ParametresEffet};
    use crate::ffb::{ControleDevice, MessageFfb, OperationEffet, TypeEffet};

    fn creer_constante(banque: &mut BanqueEffets, bloc: u8, magnitude: i32) {
        banque.appliquer(MessageFfb::NouvelEffet {
            bloc,
            type_effet: TypeEffet::Constante,
        });
        banque.appliquer(MessageFfb::Constante { bloc, magnitude });
    }

    #[test]
    fn nouvel_effet_puis_parametres() {
        let mut banque = BanqueEffets::new();
        creer_constante(&mut banque, 1, -3200);
        assert_eq!(banque.effets_en_cours().count(), 0);
        let effet = banque.effets.get(&1).expect("effet créé");
        assert_eq!(effet.type_effet, TypeEffet::Constante);
        assert_eq!(
            effet.params,
            ParametresEffet::Constante { magnitude: -3200 }
        );
    }

    #[test]
    fn demarrer_puis_arreter() {
        let mut banque = BanqueEffets::new();
        creer_constante(&mut banque, 1, 1000);
        banque.appliquer(MessageFfb::Operation {
            bloc: 1,
            operation: OperationEffet::Demarrer,
            repetitions: 1,
        });
        assert_eq!(banque.effets_en_cours().count(), 1);
        banque.appliquer(MessageFfb::Operation {
            bloc: 1,
            operation: OperationEffet::Arreter,
            repetitions: 0,
        });
        assert_eq!(banque.effets_en_cours().count(), 0);
    }

    #[test]
    fn solo_arrete_les_autres() {
        let mut banque = BanqueEffets::new();
        for bloc in 1..=3 {
            creer_constante(&mut banque, bloc, 0);
            banque.appliquer(MessageFfb::Operation {
                bloc,
                operation: OperationEffet::Demarrer,
                repetitions: 1,
            });
        }
        assert_eq!(banque.effets_en_cours().count(), 3);
        banque.appliquer(MessageFfb::Operation {
            bloc: 2,
            operation: OperationEffet::Solo,
            repetitions: 1,
        });
        let en_cours: Vec<_> = banque.effets_en_cours().collect();
        assert_eq!(en_cours.len(), 1);
    }

    #[test]
    fn arreter_tout_garde_les_effets() {
        let mut banque = BanqueEffets::new();
        creer_constante(&mut banque, 1, 0);
        banque.appliquer(MessageFfb::Operation {
            bloc: 1,
            operation: OperationEffet::Demarrer,
            repetitions: 1,
        });
        banque.appliquer(MessageFfb::Controle(ControleDevice::ArreterTout));
        assert_eq!(banque.effets_en_cours().count(), 0);
        assert!(banque.effets.contains_key(&1), "l'effet reste déclaré");
    }

    #[test]
    fn reset_vide_tout_et_reactive() {
        let mut banque = BanqueEffets::new();
        creer_constante(&mut banque, 1, 0);
        banque.appliquer(MessageFfb::Controle(ControleDevice::Desactiver));
        banque.appliquer(MessageFfb::Controle(ControleDevice::Reset));
        assert!(banque.effets.is_empty());
        assert!(banque.actif());
    }

    #[test]
    fn gain_et_activation() {
        let mut banque = BanqueEffets::new();
        banque.appliquer(MessageFfb::Gain(128));
        assert_eq!(banque.gain_global(), 128);
        banque.appliquer(MessageFfb::Controle(ControleDevice::Desactiver));
        assert!(!banque.actif());
        banque.appliquer(MessageFfb::Controle(ControleDevice::Activer));
        assert!(banque.actif());
    }

    #[test]
    fn operation_sur_bloc_absent_ne_panique_pas() {
        let mut banque = BanqueEffets::new();
        banque.appliquer(MessageFfb::Operation {
            bloc: 99,
            operation: OperationEffet::Demarrer,
            repetitions: 1,
        });
        assert_eq!(banque.effets_en_cours().count(), 0);
    }
}
