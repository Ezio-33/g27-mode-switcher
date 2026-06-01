//! Lecture et écriture de la configuration par clé textuelle.
//!
//! Sert les sous-commandes `config get <clé>` et `config set <clé> <valeur>` de
//! la CLI. La validation vit ici (et non côté CLI) pour rester l'unique source
//! de vérité, réutilisée par tous les frontaux.

use super::{ANGLE_MAX, ANGLE_MIN, Config, VERBOSITES};

/// Clés modifiables via `config set` / lisibles via `config get`.
pub const CLES_MODIFIABLES: [&str; 4] = [
    "angle_par_defaut",
    "appliquer_angle_au_switch",
    "desactiver_autocentrage_au_switch",
    "verbosite",
];

/// Erreur de lecture/écriture d'une clé de configuration.
#[derive(Debug, thiserror::Error)]
pub enum ErreurCle {
    /// La clé demandée n'existe pas.
    #[error(
        "clé inconnue : « {0} ». Clés valides : angle_par_defaut, appliquer_angle_au_switch, desactiver_autocentrage_au_switch, verbosite"
    )]
    Inconnue(String),
    /// La valeur fournie n'est pas valide pour cette clé.
    #[error("valeur invalide pour « {cle} » : attendu {attendu}")]
    ValeurInvalide {
        /// Clé concernée.
        cle: String,
        /// Description de ce qui était attendu.
        attendu: &'static str,
    },
}

impl Config {
    /// Lit la valeur d'une clé sous forme textuelle (`config get`).
    ///
    /// # Errors
    ///
    /// [`ErreurCle::Inconnue`] si la clé n'existe pas.
    pub fn lire_cle(&self, cle: &str) -> Result<String, ErreurCle> {
        match cle {
            "angle_par_defaut" => Ok(self.volant.angle_par_defaut.to_string()),
            "appliquer_angle_au_switch" => Ok(self.volant.appliquer_angle_au_switch.to_string()),
            "desactiver_autocentrage_au_switch" => {
                Ok(self.volant.desactiver_autocentrage_au_switch.to_string())
            }
            "verbosite" => Ok(self.journalisation.verbosite.clone()),
            _ => Err(ErreurCle::Inconnue(cle.to_owned())),
        }
    }

    /// Modifie la valeur d'une clé à partir d'une saisie textuelle (`config set`).
    ///
    /// # Errors
    ///
    /// [`ErreurCle::Inconnue`] si la clé n'existe pas, [`ErreurCle::ValeurInvalide`]
    /// si la valeur ne respecte pas le type ou les bornes attendus.
    pub fn definir_cle(&mut self, cle: &str, valeur: &str) -> Result<(), ErreurCle> {
        match cle {
            "angle_par_defaut" => {
                let angle: u16 = valeur
                    .parse()
                    .ok()
                    .filter(|degres| (ANGLE_MIN..=ANGLE_MAX).contains(degres))
                    .ok_or_else(|| invalide(cle, "un entier entre 40 et 900"))?;
                self.volant.angle_par_defaut = angle;
            }
            "appliquer_angle_au_switch" => {
                self.volant.appliquer_angle_au_switch = parse_bool(cle, valeur)?;
            }
            "desactiver_autocentrage_au_switch" => {
                self.volant.desactiver_autocentrage_au_switch = parse_bool(cle, valeur)?;
            }
            "verbosite" => {
                if !VERBOSITES.contains(&valeur) {
                    return Err(invalide(cle, "info, debug ou trace"));
                }
                valeur.clone_into(&mut self.journalisation.verbosite);
            }
            _ => return Err(ErreurCle::Inconnue(cle.to_owned())),
        }
        Ok(())
    }
}

/// Construit une erreur de valeur invalide.
fn invalide(cle: &str, attendu: &'static str) -> ErreurCle {
    ErreurCle::ValeurInvalide {
        cle: cle.to_owned(),
        attendu,
    }
}

/// Interprète un booléen depuis plusieurs formes (français et anglais).
fn parse_bool(cle: &str, valeur: &str) -> Result<bool, ErreurCle> {
    match valeur {
        "true" | "vrai" | "oui" | "1" => Ok(true),
        "false" | "faux" | "non" | "0" => Ok(false),
        _ => Err(invalide(cle, "true ou false")),
    }
}

#[cfg(test)]
mod tests {
    use super::super::Config;

    #[test]
    fn lire_et_definir_l_angle() {
        let mut config = Config::default();
        config
            .definir_cle("angle_par_defaut", "540")
            .expect("valeur valide");
        assert_eq!(config.volant.angle_par_defaut, 540);
        assert_eq!(config.lire_cle("angle_par_defaut").unwrap(), "540");
    }

    #[test]
    fn angle_hors_bornes_est_refuse() {
        let mut config = Config::default();
        assert!(config.definir_cle("angle_par_defaut", "2000").is_err());
        assert!(config.definir_cle("angle_par_defaut", "abc").is_err());
        // La valeur d'origine n'a pas été modifiée.
        assert_eq!(config.volant.angle_par_defaut, 900);
    }

    #[test]
    fn booleen_accepte_formes_fr_et_en() {
        let mut config = Config::default();
        config
            .definir_cle("appliquer_angle_au_switch", "faux")
            .unwrap();
        assert!(!config.volant.appliquer_angle_au_switch);
        config
            .definir_cle("appliquer_angle_au_switch", "true")
            .unwrap();
        assert!(config.volant.appliquer_angle_au_switch);
        assert!(
            config
                .definir_cle("appliquer_angle_au_switch", "peut-être")
                .is_err()
        );
    }

    #[test]
    fn verbosite_validee() {
        let mut config = Config::default();
        config.definir_cle("verbosite", "debug").unwrap();
        assert_eq!(config.journalisation.verbosite, "debug");
        assert!(config.definir_cle("verbosite", "bavard").is_err());
    }

    #[test]
    fn cle_inconnue_est_refusee() {
        let mut config = Config::default();
        assert!(config.definir_cle("inexistante", "x").is_err());
        assert!(config.lire_cle("inexistante").is_err());
    }
}
