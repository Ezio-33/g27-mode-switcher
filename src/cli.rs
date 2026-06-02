//! Définition et exécution de l'interface en ligne de commande (clap).

use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use tracing_subscriber::EnvFilter;

use g27_mode_switcher::entree::{self, EntreesG27, LecteurG27};
use g27_mode_switcher::keymapper::{self, Bouton, EtatBoutons};
use g27_mode_switcher::{autocenter, config, hid, range, switcher};

/// Bascule un volant Logitech G27 vers son mode natif, sans pilote propriétaire.
#[derive(Debug, Parser)]
#[command(name = "g27-mode-switcher", version, about, long_about = None)]
pub struct Cli {
    /// Augmente la verbosité des logs (-v : debug, -vv : trace).
    #[arg(short, long, action = ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Sans sous-commande, l'application lance son interface graphique.
    #[command(subcommand)]
    command: Option<Command>,
}

/// Sous-commandes disponibles.
#[derive(Debug, Clone, Subcommand)]
enum Command {
    /// Liste les périphériques Logitech détectés.
    List,
    /// Bascule le G27 en mode natif (retour de force complet).
    Switch {
        /// Simule l'opération : construit et valide le transfert sans rien envoyer.
        #[arg(long)]
        dry_run: bool,
        /// Ne règle pas automatiquement l'angle après la bascule.
        #[arg(long)]
        no_range: bool,
        /// Désactive l'autocentrage matériel après la bascule (laissé actif par défaut).
        #[arg(long)]
        disable_autocenter: bool,
    },
    /// Affiche le mode courant du G27 détecté.
    Status,
    /// Règle l'angle de rotation du G27 (mode natif requis), de 40° à 900°.
    SetRange {
        /// Angle de rotation souhaité, en degrés (40–900).
        degrees: u16,
    },
    /// Active ou désactive l'autocentrage matériel (mode natif requis).
    SetAutocenter {
        /// `off` désactive le ressort matériel (laisse le jeu gérer le FFB).
        state: AutocenterState,
    },
    /// Affiche ou modifie la configuration persistante.
    Config {
        /// Sans action : affiche le chemin et le contenu courant.
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Lit en direct les boutons du G27 natif (debug/calibration du keymapper).
    Boutons,
    /// Lit en direct les entrées complètes du G27 (axes + boutons, debug feeder vJoy).
    Entrees,
}

/// Actions de la sous-commande `config`.
#[derive(Debug, Clone, Subcommand)]
enum ConfigAction {
    /// Affiche la valeur d'une clé.
    Get {
        /// Clé à lire (ex. `angle_par_defaut`).
        cle: String,
    },
    /// Modifie la valeur d'une clé et enregistre la configuration.
    Set {
        /// Clé à modifier (ex. `angle_par_defaut`).
        cle: String,
        /// Nouvelle valeur.
        valeur: String,
    },
}

/// État cible de l'autocentrage matériel.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum AutocenterState {
    /// Désactive l'autocentrage matériel.
    Off,
    /// Réactive l'autocentrage matériel à pleine force.
    On,
}

impl Cli {
    /// Exécute la sous-commande choisie, ou lance la GUI si aucune n'est fournie.
    #[must_use]
    pub fn run(self) -> ExitCode {
        match self.command {
            Some(command) => {
                let config = config::Config::charger();
                init_cli_logging(self.verbose, &config.journalisation.verbosite);
                dispatch(command, config)
            }
            None => crate::gui::run(self.verbose),
        }
    }
}

/// Initialise le logging CLI (sortie texte vers le terminal).
///
/// Précédence du niveau : `RUST_LOG` > `-v`/`-vv` > `config_level` > défaut.
fn init_cli_logging(verbose: u8, config_level: &str) {
    let default_level = match verbose {
        0 => config_level,
        1 => "debug",
        _ => "trace",
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Exécute une sous-commande CLI et renvoie son code de sortie.
fn dispatch(command: Command, config: config::Config) -> ExitCode {
    match command {
        Command::List => run_list(),
        Command::Switch {
            dry_run,
            no_range,
            disable_autocenter,
        } => run_switch(dry_run, no_range, disable_autocenter, &config),
        Command::Status => run_status(),
        Command::SetRange { degrees } => run_set_range(degrees),
        Command::SetAutocenter { state } => run_set_autocenter(state),
        Command::Config { action } => run_config(action, config),
        Command::Boutons => run_boutons(),
        Command::Entrees => run_entrees(),
    }
}

/// Liste les périphériques Logitech détectés.
fn run_list() -> ExitCode {
    match hid::list_logitech_devices() {
        Ok(devices) if devices.is_empty() => {
            println!("Aucun périphérique Logitech détecté.");
            ExitCode::SUCCESS
        }
        Ok(devices) => {
            println!("Périphériques Logitech détectés ({}) :", devices.len());
            for device in &devices {
                println!("  - {device}");
            }
            let g27_count = devices.iter().filter(|device| device.is_g27()).count();
            if g27_count > 0 {
                println!();
                println!("Volant(s) G27 détecté(s) : {g27_count}.");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Erreur : {error}");
            ExitCode::FAILURE
        }
    }
}

/// Bascule un G27 en mode natif (ou simule l'opération en `--dry-run`).
///
/// L'angle appliqué et l'autocentrage proviennent de la configuration ; les
/// options explicites restent prioritaires : `--no-range` désactive le réglage
/// d'angle, `--disable-autocenter` force la désactivation de l'autocentrage.
fn run_switch(
    dry_run: bool,
    no_range: bool,
    disable_autocenter: bool,
    config: &config::Config,
) -> ExitCode {
    let apply_range = !no_range && config.volant.appliquer_angle_au_switch;
    let range_degrees = config.volant.angle_par_defaut;
    let disable = disable_autocenter || config.volant.desactiver_autocentrage_au_switch;

    if dry_run {
        println!("Simulation (--dry-run) : aucune donnée ne sera envoyée au volant.");
    } else if apply_range {
        println!("Bascule du G27 vers le mode natif (angle réglé à {range_degrees}°)...");
        println!("Le volant va se déconnecter puis se reconnecter automatiquement.");
    } else {
        println!("Bascule du G27 vers le mode natif (sans réglage d'angle)...");
        println!("Le volant va se déconnecter puis se reconnecter automatiquement.");
    }

    match switcher::switch_to_native_mode(dry_run, apply_range, range_degrees, disable) {
        Ok(outcome) if outcome.dry_run => {
            println!("Simulation OK : G27 éligible détecté → {}", outcome.device);
            ExitCode::SUCCESS
        }
        Ok(outcome) => {
            println!(
                "Magic packet envoyé au {}. Il va réapparaître en mode natif.",
                outcome.device
            );
            report_range_step(outcome.range);
            report_autocenter_step(outcome.autocenter);
            ExitCode::SUCCESS
        }
        Err(switcher::Error::NoG27Found) => {
            eprintln!("Aucun G27 détecté. Branchez le volant puis réessayez.");
            ExitCode::FAILURE
        }
        Err(switcher::Error::AlreadyNative) => {
            println!("Le G27 est déjà en mode natif : rien à faire.");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Échec de la bascule : {error}");
            ExitCode::FAILURE
        }
    }
}

/// Affiche à l'utilisateur l'issue du réglage automatique de l'angle.
fn report_range_step(step: switcher::RangeStep) {
    match step {
        switcher::RangeStep::Skipped => {}
        switcher::RangeStep::Applied(degrees) => {
            println!("Angle de rotation réglé automatiquement sur {degrees}°.");
        }
        switcher::RangeStep::Deferred(degrees) => {
            println!(
                "Bascule réussie, mais l'angle n'a pas pu être réglé automatiquement. Une fois le volant reconnecté, lancez : set-range {degrees}"
            );
        }
    }
}

/// Affiche à l'utilisateur l'issue de la désactivation automatique de l'autocentrage.
fn report_autocenter_step(step: switcher::AutocenterStep) {
    match step {
        switcher::AutocenterStep::Skipped => {}
        switcher::AutocenterStep::Disabled => {
            println!(
                "Autocentrage matériel désactivé. Sans couche FFB active, le volant n'aura plus de force de centrage."
            );
        }
        switcher::AutocenterStep::Deferred => {
            println!(
                "Bascule réussie, mais l'autocentrage n'a pas pu être désactivé automatiquement. Une fois le volant reconnecté, lancez : set-autocenter off"
            );
        }
    }
}

/// Affiche le mode courant du G27 détecté.
fn run_status() -> ExitCode {
    match hid::list_logitech_devices() {
        Ok(devices) => {
            let mode = devices
                .iter()
                .find(|device| device.is_g27())
                .map(|device| device.mode);
            match mode {
                Some(hid::G27Mode::Native) => {
                    println!("G27 détecté en mode natif (900°, retour de force complet).");
                }
                Some(hid::G27Mode::Compatibility) => {
                    println!(
                        "G27 détecté en mode compatibilité (200°). Lancez « switch » pour basculer en mode natif."
                    );
                }
                Some(hid::G27Mode::Other) | None => {
                    println!("Aucun G27 détecté.");
                }
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Erreur : {error}");
            ExitCode::FAILURE
        }
    }
}

/// Règle l'angle de rotation du G27 (mode natif requis).
fn run_set_range(degrees: u16) -> ExitCode {
    match range::set_range(degrees) {
        Ok(outcome) => {
            println!(
                "Angle de rotation réglé sur {}° pour le {}.",
                outcome.degrees, outcome.device
            );
            ExitCode::SUCCESS
        }
        Err(range::Error::OutOfRange(value)) => {
            eprintln!("Angle invalide : {value}°. Indiquez une valeur entre 40 et 900 degrés.");
            ExitCode::FAILURE
        }
        Err(range::Error::NotNative) => {
            eprintln!(
                "Le G27 est en mode compatibilité : le réglage d'angle n'a aucun effet. Lancez « switch » d'abord."
            );
            ExitCode::FAILURE
        }
        Err(range::Error::NoG27Found) => {
            eprintln!("Aucun G27 détecté. Branchez le volant puis réessayez.");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("Échec du réglage d'angle : {error}");
            ExitCode::FAILURE
        }
    }
}

/// Désactive (`off`) ou réactive (`on`) l'autocentrage matériel du G27 natif.
fn run_set_autocenter(state: AutocenterState) -> ExitCode {
    let resultat = match state {
        AutocenterState::Off => autocenter::disable_autocenter(),
        AutocenterState::On => autocenter::enable_autocenter(),
    };
    match resultat {
        Ok(outcome) => {
            match state {
                AutocenterState::Off => println!(
                    "Autocentrage matériel désactivé pour le {}. Sans couche FFB active, le volant n'aura plus de force de centrage.",
                    outcome.device
                ),
                AutocenterState::On => println!(
                    "Autocentrage matériel réactivé (pleine force) pour le {}.",
                    outcome.device
                ),
            }
            ExitCode::SUCCESS
        }
        Err(autocenter::Error::NotNative) => {
            eprintln!(
                "Le G27 est en mode compatibilité : la commande n'a aucun effet. Lancez « switch » d'abord."
            );
            ExitCode::FAILURE
        }
        Err(autocenter::Error::NoG27Found) => {
            eprintln!("Aucun G27 détecté. Branchez le volant puis réessayez.");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("Échec de la commande d'autocentrage : {error}");
            ExitCode::FAILURE
        }
    }
}

/// Affiche ou modifie la configuration persistante.
fn run_config(action: Option<ConfigAction>, config: config::Config) -> ExitCode {
    match action {
        None => run_config_show(&config),
        Some(ConfigAction::Get { cle }) => run_config_get(&config, &cle),
        Some(ConfigAction::Set { cle, valeur }) => run_config_set(config, &cle, &valeur),
    }
}

/// Affiche le chemin et le contenu courant de la configuration.
fn run_config_show(config: &config::Config) -> ExitCode {
    match config::chemin() {
        Some(chemin) => println!("Fichier de configuration : {}", chemin.display()),
        None => println!("Fichier de configuration : introuvable sur ce système."),
    }
    println!();
    print!("{}", config.vers_toml());
    ExitCode::SUCCESS
}

/// Affiche la valeur d'une clé de configuration.
fn run_config_get(config: &config::Config, cle: &str) -> ExitCode {
    match config.lire_cle(cle) {
        Ok(valeur) => {
            println!("{valeur}");
            ExitCode::SUCCESS
        }
        Err(erreur) => {
            eprintln!("Erreur : {erreur}");
            ExitCode::FAILURE
        }
    }
}

/// Ouvre le G27 natif et affiche en direct ses rapports (debug/calibration).
///
/// Boucle jusqu'à interruption (Ctrl+C) ; `formatter` produit la ligne affichée à
/// partir du rapport brut, et la ligne n'est réimprimée que lorsqu'elle change.
fn boucle_lecture(en_tete: &str, formatter: impl Fn(&[u8]) -> String) -> ExitCode {
    println!("{en_tete}\n");

    let api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(erreur) => {
            eprintln!("Erreur : accès HID impossible : {erreur}");
            return ExitCode::FAILURE;
        }
    };
    let mut lecteur = match LecteurG27::ouvrir(&api) {
        Ok(lecteur) => lecteur,
        Err(erreur) => {
            eprintln!("Erreur : {erreur}");
            return ExitCode::FAILURE;
        }
    };

    let mut derniere = String::new();
    loop {
        match lecteur.lire(200) {
            Ok(true) => {
                let ligne = formatter(lecteur.rapport());
                if ligne != derniere {
                    println!("{ligne}");
                    derniere = ligne;
                }
            }
            Ok(false) => {}
            Err(erreur) => {
                eprintln!("Erreur de lecture : {erreur}");
                return ExitCode::FAILURE;
            }
        }
    }
}

/// Lit en direct les boutons du G27 (debug/calibration du keymapper).
///
/// Sert à caler `OCTET_DEBUT_BOUTONS` : affiche le rapport brut, les bits armés
/// (indices absolus) et les rapports de boîte H reconnus.
fn run_boutons() -> ExitCode {
    let en_tete = format!(
        "Lecture des boutons du G27 natif (Ctrl+C pour quitter).\n\
         Boutons HID attendus : 1re=13 2e=14 3e=15 4e=16 5e=17 6e=18 MA=23 ; lus au bit (n-1) depuis l'octet {}.\n\
         Engagez chaque rapport pour repérer le bit qui s'arme.",
        keymapper::OCTET_DEBUT_BOUTONS
    );
    boucle_lecture(&en_tete, |rapport| {
        format_rapport(rapport, keymapper::boutons_depuis_rapport(rapport))
    })
}

/// Lit en direct les entrées complètes du G27 (debug/calibration du feeder vJoy).
///
/// Sert à caler les offsets d'axes : affiche le rapport brut et les valeurs
/// décodées (volant, pédales, chapeau, boutons).
fn run_entrees() -> ExitCode {
    boucle_lecture(
        "Lecture des entrées du G27 natif (Ctrl+C pour quitter).\n\
         Bougez le volant et les pédales pour caler les offsets d'axes.",
        |rapport| format_entrees(rapport, entree::entrees_depuis_rapport(rapport)),
    )
}

/// Formate un rapport HID pour la calibration : hex, bits armés, rapports reconnus.
fn format_rapport(rapport: &[u8], etat: EtatBoutons) -> String {
    let hex: Vec<String> = rapport.iter().map(|octet| format!("{octet:02x}")).collect();
    let engages: Vec<&str> = Bouton::TOUS
        .iter()
        .filter(|bouton| etat.contient(**bouton))
        .map(|bouton| bouton.libelle())
        .collect();
    let engages = if engages.is_empty() {
        "—".to_owned()
    } else {
        engages.join(", ")
    };
    format!(
        "[{hex}]  bits = {bits:?}  →  rapports : {engages}",
        hex = hex.join(" "),
        bits = bits_armes(rapport),
    )
}

/// Indices absolus (depuis l'octet 0) des bits armés dans le rapport.
fn bits_armes(rapport: &[u8]) -> Vec<usize> {
    let mut bits = Vec::new();
    for (index_octet, octet) in rapport.iter().enumerate() {
        for bit in 0u8..8 {
            if octet & (1u8 << bit) != 0 {
                bits.push(index_octet * 8 + usize::from(bit));
            }
        }
    }
    bits
}

/// Formate les entrées décodées du G27 pour la calibration des axes.
fn format_entrees(rapport: &[u8], entrees: EntreesG27) -> String {
    let hex: Vec<String> = rapport.iter().map(|octet| format!("{octet:02x}")).collect();
    let boutons: Vec<u8> = (1u8..=24)
        .filter(|numero| entrees.boutons.est_presse(*numero))
        .collect();
    format!(
        "[{hex}]  volant={volant:5}  acc={acc:3}  frein={frein:3}  embr={embr:3}  chapeau={chapeau}  boutons={boutons:?}",
        hex = hex.join(" "),
        volant = entrees.volant,
        acc = entrees.accelerateur,
        frein = entrees.frein,
        embr = entrees.embrayage,
        chapeau = entrees.chapeau,
    )
}

/// Modifie une clé de configuration puis enregistre le fichier.
fn run_config_set(mut config: config::Config, cle: &str, valeur: &str) -> ExitCode {
    if let Err(erreur) = config.definir_cle(cle, valeur) {
        eprintln!("Erreur : {erreur}");
        return ExitCode::FAILURE;
    }
    match config.enregistrer() {
        Ok(()) => {
            println!("Réglage « {cle} » = {valeur} enregistré.");
            ExitCode::SUCCESS
        }
        Err(erreur) => {
            eprintln!("Échec de l'enregistrement de la configuration : {erreur}");
            ExitCode::FAILURE
        }
    }
}
