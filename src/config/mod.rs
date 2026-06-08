//! Persistance de la configuration utilisateur au format TOML.
//!
//! La configuration est lue au démarrage et réécrite à chaque changement. Le
//! chargement est **tolérant** : un fichier absent ou un TOML invalide ne bloque
//! jamais l'application — on retombe sur les valeurs par défaut (avec un
//! avertissement `tracing`). Les valeurs lues sont **assainies** (angle borné,
//! verbosité connue, géométrie cohérente). L'écriture est **atomique** (fichier
//! temporaire puis renommage) pour éviter toute corruption.
//!
//! Les structs et les clés TOML suivent une nomenclature **française** (sans
//! accents, pour rester des identifiants Rust et des clés TOML valides).

mod cles;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use cles::{CLES_MODIFIABLES, ErreurCle};

/// Angle de rotation minimal accepté (degrés), cf. `lg4ff_set_range_g25`.
const ANGLE_MIN: u16 = 40;
/// Angle de rotation maximal accepté (degrés).
const ANGLE_MAX: u16 = 900;
/// Angle de rotation appliqué par défaut après une bascule.
const ANGLE_DEFAUT: u16 = 900;
/// Largeur par défaut de la fenêtre (px logiques).
const LARGEUR_DEFAUT: f32 = 480.0;
/// Hauteur par défaut de la fenêtre (px logiques).
const HAUTEUR_DEFAUT: f32 = 800.0;
/// Niveau de verbosité par défaut.
const VERBOSITE_DEFAUT: &str = "info";
/// Niveaux de verbosité reconnus.
const VERBOSITES: [&str; 3] = ["info", "debug", "trace"];
/// Identifiant de device vJoy minimal.
const ID_VJOY_MIN: u32 = 1;
/// Identifiant de device vJoy maximal.
const ID_VJOY_MAX: u32 = 16;
/// Identifiant de device vJoy par défaut.
const ID_VJOY_DEFAUT: u32 = 1;

/// Configuration complète de l'application.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Réglages liés au volant.
    pub volant: Volant,
    /// Géométrie de la fenêtre graphique.
    pub fenetre: Fenetre,
    /// Réglages de journalisation.
    pub journalisation: Journalisation,
    /// Réglages du pont vJoy.
    pub pont: Pont,
}

/// Réglages liés au volant (section `[volant]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Volant {
    /// Angle de rotation appliqué par défaut après une bascule (40–900).
    pub angle_par_defaut: u16,
    /// Régler automatiquement l'angle lors d'un `switch`.
    pub appliquer_angle_au_switch: bool,
    /// Désactiver l'autocentrage matériel lors d'un `switch`.
    pub desactiver_autocentrage_au_switch: bool,
    /// Mode du volant souhaité, **restauré au démarrage de la GUI**. Le firmware
    /// revient en compatibilité à chaque cycle USB : si l'utilisateur était en natif,
    /// on rebascule automatiquement ; s'il préférait la compatibilité, on n'y touche
    /// pas. Mis à jour à chaque action de bascule de l'utilisateur.
    pub mode_souhaite: ModeSouhaite,
}

/// Mode du volant mémorisé entre deux sessions (préférence utilisateur).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModeSouhaite {
    /// Restaurer le mode natif : bascule automatique si le volant est en compat.
    #[default]
    Natif,
    /// Conserver le mode compatibilité : aucune bascule automatique.
    Compatibilite,
}

impl ModeSouhaite {
    /// Représentation textuelle stable (clés `config get`/`set`, sérialisation).
    #[must_use]
    pub fn comme_str(self) -> &'static str {
        match self {
            Self::Natif => "natif",
            Self::Compatibilite => "compatibilite",
        }
    }

    /// Interprète une saisie textuelle (formes FR/EN tolérées).
    #[must_use]
    pub fn depuis_str(valeur: &str) -> Option<Self> {
        match valeur {
            "natif" | "native" => Some(Self::Natif),
            "compatibilite" | "compat" | "compatibility" => Some(Self::Compatibilite),
            _ => None,
        }
    }
}

/// Géométrie de la fenêtre graphique (section `[fenetre]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fenetre {
    /// Largeur de la fenêtre (px logiques).
    pub largeur: f32,
    /// Hauteur de la fenêtre (px logiques).
    pub hauteur: f32,
    /// Position horizontale (absente au premier lancement).
    pub pos_x: Option<f32>,
    /// Position verticale (absente au premier lancement).
    pub pos_y: Option<f32>,
}

/// Réglages de journalisation (section `[journalisation]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Journalisation {
    /// Verbosité par défaut : `info`, `debug` ou `trace`.
    pub verbosite: String,
}

/// Réglages du pont vJoy (section `[pont]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pont {
    /// Identifiant du device vJoy alimenté (1–16).
    pub id_vjoy: u32,
    /// Masquer le G27 réel au jeu au démarrage du pont.
    pub masquer_g27_au_demarrage: bool,
    /// Couper l'autocentrage matériel pendant le pont FFB. Par défaut **non** :
    /// le FFB ne fournit que la force constante (nulle à l'arrêt), donc on garde
    /// le ressort firmware actif pour conserver une résistance/centrage à l'arrêt
    /// (« friction des pneus »). Ce ressort est open-loop : aucun risque d'oscillation.
    /// À mettre à `true` seulement si une couche FFB complète prend le relais.
    pub couper_autocentrage_ffb: bool,
    /// Traduire le D-pad du G27 en **flèches clavier** pendant le pont. Utile quand le
    /// G27 est masqué (pour le FFB) : certains jeux (Forza) ne naviguent leurs menus
    /// qu'au clavier ou avec un volant reconnu, pas avec un device vJoy générique.
    /// Effet de bord : frappes clavier globales (fenêtre au premier plan).
    pub chapeau_vers_clavier: bool,
}

impl Default for Pont {
    fn default() -> Self {
        Self {
            id_vjoy: ID_VJOY_DEFAUT,
            masquer_g27_au_demarrage: true,
            couper_autocentrage_ffb: false,
            chapeau_vers_clavier: false,
        }
    }
}

impl Default for Volant {
    fn default() -> Self {
        Self {
            angle_par_defaut: ANGLE_DEFAUT,
            appliquer_angle_au_switch: true,
            desactiver_autocentrage_au_switch: false,
            mode_souhaite: ModeSouhaite::Natif,
        }
    }
}

impl Default for Fenetre {
    fn default() -> Self {
        Self {
            largeur: LARGEUR_DEFAUT,
            hauteur: HAUTEUR_DEFAUT,
            pos_x: None,
            pos_y: None,
        }
    }
}

impl Default for Journalisation {
    fn default() -> Self {
        Self {
            verbosite: VERBOSITE_DEFAUT.to_owned(),
        }
    }
}

/// Erreur survenant lors de l'écriture de la configuration.
#[derive(Debug, thiserror::Error)]
pub enum Erreur {
    /// Le dossier de configuration n'a pas pu être déterminé.
    #[error("dossier de configuration introuvable sur ce système")]
    DossierIntrouvable,
    /// Échec d'accès disque (création de dossier, écriture, renommage).
    #[error("accès disque impossible : {0}")]
    Io(#[from] std::io::Error),
    /// Échec de sérialisation TOML.
    #[error("sérialisation TOML impossible : {0}")]
    Serialisation(#[from] toml::ser::Error),
}

impl Config {
    /// Charge la configuration depuis le disque, en retombant sur les valeurs par
    /// défaut si le fichier est absent, illisible ou invalide.
    #[must_use]
    pub fn charger() -> Self {
        let Some(chemin) = chemin() else {
            tracing::warn!("Dossier de configuration introuvable ; réglages par défaut utilisés.");
            return Self::default();
        };
        match std::fs::read_to_string(&chemin) {
            Ok(contenu) => depuis_toml(&contenu),
            Err(erreur) if erreur.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(erreur) => {
                tracing::warn!(%erreur, "Lecture de la configuration impossible ; réglages par défaut utilisés.");
                Self::default()
            }
        }
    }

    /// Enregistre la configuration sur le disque de façon atomique.
    ///
    /// # Errors
    ///
    /// Renvoie une [`Erreur`] si le dossier de configuration est introuvable, si
    /// la sérialisation TOML échoue, ou en cas d'erreur d'accès disque.
    pub fn enregistrer(&self) -> Result<(), Erreur> {
        let chemin = chemin().ok_or(Erreur::DossierIntrouvable)?;
        if let Some(parent) = chemin.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contenu = toml::to_string_pretty(self)?;
        // Écriture atomique : on écrit dans un fichier temporaire du même dossier
        // puis on le renomme (le rename est atomique sur la même partition).
        let temporaire = chemin.with_extension("toml.tmp");
        std::fs::write(&temporaire, contenu)?;
        std::fs::rename(&temporaire, &chemin)?;
        Ok(())
    }

    /// Rend la configuration sous forme de texte TOML (pour l'affichage CLI).
    #[must_use]
    pub fn vers_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Borne et normalise les valeurs lues pour qu'elles restent cohérentes.
    fn assainir(&mut self) {
        self.volant.angle_par_defaut = self.volant.angle_par_defaut.clamp(ANGLE_MIN, ANGLE_MAX);
        if !VERBOSITES.contains(&self.journalisation.verbosite.as_str()) {
            VERBOSITE_DEFAUT.clone_into(&mut self.journalisation.verbosite);
        }
        if self.fenetre.largeur <= 0.0 {
            self.fenetre.largeur = LARGEUR_DEFAUT;
        }
        if self.fenetre.hauteur <= 0.0 {
            self.fenetre.hauteur = HAUTEUR_DEFAUT;
        }
        self.pont.id_vjoy = self.pont.id_vjoy.clamp(ID_VJOY_MIN, ID_VJOY_MAX);
    }
}

/// Désérialise et assainit une configuration ; un TOML invalide donne les défauts.
fn depuis_toml(contenu: &str) -> Config {
    match toml::from_str::<Config>(contenu) {
        Ok(mut config) => {
            config.assainir();
            config
        }
        Err(erreur) => {
            tracing::warn!(%erreur, "Configuration illisible (TOML invalide) ; réglages par défaut utilisés.");
            Config::default()
        }
    }
}

/// Nom du dossier applicatif sous le répertoire de configuration.
const NOM_APP: &str = "g27-mode-switcher";

/// Chemin du fichier de configuration (`<dossier_config>/config.toml`).
///
/// - Windows : `%APPDATA%\g27-mode-switcher\config.toml` ;
/// - autres : `$XDG_CONFIG_HOME/g27-mode-switcher/config.toml`, sinon
///   `~/.config/g27-mode-switcher/config.toml`.
#[must_use]
pub fn chemin() -> Option<PathBuf> {
    dossier_config().map(|dossier| dossier.join("config.toml"))
}

/// Dossier de configuration de l'application, résolu via les variables
/// d'environnement standard de l'OS (sans dépendance tierce).
fn dossier_config() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|appdata| PathBuf::from(appdata).join(NOM_APP))
    }
    #[cfg(not(windows))]
    {
        dossier_config_unix(
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("HOME"),
        )
    }
}

/// Logique de résolution POSIX (pure, testable) : `XDG_CONFIG_HOME` non vide,
/// sinon `~/.config`.
#[cfg(not(windows))]
fn dossier_config_unix(
    xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(xdg) = xdg_config_home.filter(|valeur| !valeur.is_empty()) {
        Some(PathBuf::from(xdg).join(NOM_APP))
    } else {
        home.map(|home| PathBuf::from(home).join(".config").join(NOM_APP))
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, depuis_toml};

    #[cfg(not(windows))]
    #[test]
    fn chemin_unix_prefere_xdg_config_home() {
        use std::ffi::OsString;
        use std::path::PathBuf;
        let dossier = super::dossier_config_unix(
            Some(OsString::from("/tmp/xdg")),
            Some(OsString::from("/home/u")),
        );
        assert_eq!(dossier, Some(PathBuf::from("/tmp/xdg/g27-mode-switcher")));
    }

    #[cfg(not(windows))]
    #[test]
    fn chemin_unix_repli_sur_home_config() {
        use std::ffi::OsString;
        use std::path::PathBuf;
        let attendu = Some(PathBuf::from("/home/u/.config/g27-mode-switcher"));
        // XDG absent → repli sur ~/.config.
        assert_eq!(
            super::dossier_config_unix(None, Some(OsString::from("/home/u"))),
            attendu
        );
        // XDG défini mais vide → repli également.
        assert_eq!(
            super::dossier_config_unix(Some(OsString::new()), Some(OsString::from("/home/u"))),
            attendu
        );
    }

    #[test]
    fn defaut_fait_un_aller_retour_toml() {
        let config = Config::default();
        let texte = toml::to_string_pretty(&config).expect("sérialisation");
        assert_eq!(depuis_toml(&texte), config);
    }

    #[test]
    fn angle_hors_bornes_est_borne() {
        assert_eq!(
            depuis_toml("[volant]\nangle_par_defaut = 2000")
                .volant
                .angle_par_defaut,
            900
        );
        assert_eq!(
            depuis_toml("[volant]\nangle_par_defaut = 10")
                .volant
                .angle_par_defaut,
            40
        );
    }

    #[test]
    fn verbosite_inconnue_revient_a_info() {
        assert_eq!(
            depuis_toml("[journalisation]\nverbosite = \"bavard\"")
                .journalisation
                .verbosite,
            "info"
        );
    }

    #[test]
    fn toml_partiel_complete_avec_les_defauts() {
        let config = depuis_toml("[volant]\nappliquer_angle_au_switch = false");
        assert!(!config.volant.appliquer_angle_au_switch);
        // Les champs absents reprennent les valeurs par défaut.
        assert_eq!(config.volant.angle_par_defaut, 900);
        assert!((config.fenetre.largeur - 480.0).abs() < f32::EPSILON);
    }

    #[test]
    fn toml_invalide_donne_les_defauts() {
        assert_eq!(depuis_toml("ceci n'est pas du toml ==="), Config::default());
    }

    #[test]
    fn geometrie_invalide_est_corrigee() {
        let config = depuis_toml("[fenetre]\nlargeur = 0.0\nhauteur = -5.0");
        assert!((config.fenetre.largeur - 480.0).abs() < f32::EPSILON);
        assert!((config.fenetre.hauteur - 800.0).abs() < f32::EPSILON);
    }
}
