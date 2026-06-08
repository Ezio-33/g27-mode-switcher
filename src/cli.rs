//! Définition et exécution de l'interface en ligne de commande (clap).

use std::io::Write as _;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use tracing_subscriber::EnvFilter;

use g27_mode_switcher::entree::{self, EntreesG27, LecteurG27};
use g27_mode_switcher::keymapper::{self, Bouton, EtatBoutons};
use g27_mode_switcher::{autocenter, config, ffb, hid, hidhide, pont, range, report, switcher};

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
    /// Calibration de la boîte en H : affiche la position X/Y du levier par vitesse.
    Levier,
    /// Vide le descripteur de rapport HID de chaque interface du G27 natif (debug).
    Descripteur,
    /// Démarre le pont (feeder vJoy + masquage du G27). Ctrl+C pour arrêter.
    Feeder {
        /// Device vJoy à alimenter (1–16). Défaut : `id_vjoy` de la config.
        #[arg(long)]
        id: Option<u32>,
        /// Ne pas masquer le G27 au jeu (feeder seul).
        #[arg(long)]
        sans_masquage: bool,
    },
    /// Masque ou démasque le G27 réel au jeu via `HidHide` (debug FFB).
    Hidhide {
        /// Action à effectuer.
        #[command(subcommand)]
        action: HidhideAction,
    },
    /// Pont vJoy : diagnostic des prérequis (vJoy + `HidHide`).
    Pont {
        /// Action à effectuer.
        #[command(subcommand)]
        action: PontAction,
    },
    /// Retour de force (FFB) : outils de diagnostic du pont FFB (Phase 5).
    Ffb {
        /// Action à effectuer.
        #[command(subcommand)]
        action: FfbAction,
    },
}

/// Actions de la sous-commande `pont`.
#[derive(Debug, Clone, Copy, Subcommand)]
enum PontAction {
    /// Affiche l'état des prérequis du pont (vJoy + `HidHide`).
    Statut,
}

/// Actions de la sous-commande `ffb`.
#[derive(Debug, Clone, Subcommand)]
enum FfbAction {
    /// Capture les paquets FFB que le jeu envoie au device vJoy (debug).
    /// Le device doit avoir « Enable Effects » activé. Arrêt : fermez la console.
    Capturer {
        /// Device vJoy à écouter (1–16). Défaut : `id_vjoy` de la config.
        #[arg(long)]
        id: Option<u32>,
        /// Enregistre les paquets dans ce fichier (un par ligne, vidé à chaque écriture).
        /// À privilégier : la redirection shell `> fichier` est inopérante avec la
        /// console hybride de l'application (sortie réattachée à la console).
        #[arg(long)]
        fichier: Option<String>,
    },
    /// Démarre le pont FFB complet : feeder vJoy + masquage + retour de force du jeu
    /// vers le G27 (autocentrage coupé). Arrêt : fermez la console (volant remis au neutre).
    Pont {
        /// Device vJoy à alimenter (1–16). Défaut : `id_vjoy` de la config.
        #[arg(long)]
        id: Option<u32>,
        /// Ne pas masquer le G27 au jeu (pont FFB seul).
        #[arg(long)]
        sans_masquage: bool,
    },
    /// Applique une force constante au G27 natif et la maintient (test FFB Phase 5).
    /// ⚠️ SÉCURITÉ : commencez par de TRÈS PETITES valeurs. Arrêt : fermez la console.
    Force {
        /// Couple à appliquer (−10000..10000 ; négatif = gauche, positif = droite).
        /// `allow_hyphen_values` permet de passer une valeur négative sans `--`.
        #[arg(allow_hyphen_values = true)]
        valeur: i32,
    },
}

/// Actions de la sous-commande `hidhide`.
#[derive(Debug, Clone, Copy, Subcommand)]
enum HidhideAction {
    /// Affiche la disponibilité de `HidHide`.
    Statut,
    /// Active/désactive le masquage seul (test isolé de `SET_ACTIVE`).
    Actif {
        /// `on` active, `off` désactive.
        etat: EtatActif,
    },
    /// Masque le G27 réel au jeu (cet exécutable reste autorisé à le lire).
    Masquer,
    /// Démasque le G27 réel (de nouveau visible de toutes les applications).
    Demasquer,
}

/// État cible du masquage (sous-commande `hidhide actif`).
#[derive(Debug, Clone, Copy, ValueEnum)]
enum EtatActif {
    /// Active le masquage.
    On,
    /// Désactive le masquage.
    Off,
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
                dispatch(command, config, self.verbose)
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
fn dispatch(command: Command, config: config::Config, verbose: u8) -> ExitCode {
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
        Command::Entrees => run_entrees(verbose),
        Command::Levier => run_levier(),
        Command::Descripteur => run_descripteur(),
        Command::Feeder { id, sans_masquage } => run_feeder(id, sans_masquage),
        Command::Hidhide { action } => run_hidhide(action),
        Command::Pont { action } => run_pont(action),
        Command::Ffb { action } => run_ffb(action),
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
/// `cle` produit la signature de déduplication : une nouvelle ligne n'est imprimée
/// que lorsque cette signature change. Cela permet d'ignorer le bruit des axes
/// (volant/pédales qui tremblent de ±1) pour ne réagir qu'aux vrais changements.
fn boucle_lecture(
    en_tete: &str,
    cle: impl Fn(&[u8]) -> Vec<u8>,
    formatter: impl Fn(&[u8]) -> String,
) -> ExitCode {
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

    let mut derniere = Vec::new();
    loop {
        match lecteur.lire(200) {
            Ok(true) => {
                let signature = cle(lecteur.rapport());
                if signature != derniere {
                    println!("{}", formatter(lecteur.rapport()));
                    derniere = signature;
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

/// Octets purement analogiques et très bruyants : volant little-endian (3, 4) et
/// position X/Y du levier de la boîte H (8, 9). Validé matériel : le levier H du G27
/// n'envoie pas des boutons mais deux axes continus qui inondent l'affichage. On les
/// neutralise dans la clé de déduplication pour ne révéler que les vrais boutons.
const OCTETS_ANALOGIQUES_BRUYANTS: [usize; 4] = [3, 4, 8, 9];

/// Clé de déduplication « boutons seuls » : ne garde que la portion non bruitée du
/// rapport. Les octets de boutons (0–2) gardent leur pleine résolution ; les octets
/// analogiques bruyants ([`OCTETS_ANALOGIQUES_BRUYANTS`]) sont effacés ; les autres
/// octets (≥3) sont réduits à leur quartet haut pour absorber le tremblement de ±1.
/// Une nouvelle ligne ne s'imprime donc que lorsqu'un vrai bouton change.
fn cle_boutons(rapport: &[u8]) -> Vec<u8> {
    rapport
        .iter()
        .enumerate()
        .map(|(index, &octet)| {
            if index < 3 {
                octet
            } else if OCTETS_ANALOGIQUES_BRUYANTS.contains(&index) {
                0
            } else {
                octet & 0xF0
            }
        })
        .collect()
}

/// Clé de déduplication « élargie » : n'efface **aucun** octet (contrairement à
/// [`cle_boutons`]), mais masque le quartet bas des octets ≥3 pour absorber le
/// tremblement analogique. Révèle un bouton caché dans un octet d'axe (volant ou
/// levier H), à condition de tenir ces axes immobiles pendant la capture.
fn cle_boutons_large(rapport: &[u8]) -> Vec<u8> {
    rapport
        .iter()
        .enumerate()
        .map(|(index, &octet)| if index < 3 { octet } else { octet & 0xF0 })
        .collect()
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
    boucle_lecture(&en_tete, cle_boutons, |rapport| {
        format_rapport(rapport, keymapper::boutons_depuis_rapport(rapport))
    })
}

/// Lit en direct les entrées complètes du G27 (debug/calibration du feeder vJoy).
///
/// Sert à caler les offsets d'axes : affiche le rapport brut et les valeurs
/// décodées (volant, pédales, chapeau, boutons). En mode `-v`, ajoute le détail
/// **brut → mappé** des boutons pour localiser un bit ignoré par le parsing : tous
/// les bits armés du rapport (indices absolus) et les boutons réellement recopiés
/// vers vJoy. Un bit qui s'arme sans apparaître côté vJoy pointe l'octet/bit manquant.
fn run_entrees(verbose: u8) -> ExitCode {
    let en_tete = match verbose {
        0 => {
            "Lecture des entrées du G27 natif (Ctrl+C pour quitter).\n\
             Bougez le volant et les pédales pour caler les offsets d'axes."
        }
        1 => {
            "Lecture des entrées du G27 natif (Ctrl+C pour quitter) — diagnostic boutons.\n\
             Presse un bouton manquant : un nouvel octet:bit apparaît hors des axes analogiques."
        }
        _ => {
            "Lecture des entrées du G27 natif (Ctrl+C pour quitter) — diagnostic ÉLARGI.\n\
             Tiens le volant ET le levier IMMOBILES (point mort), puis presse le bouton cherché :\n\
             rien n'est effacé (seul le tremblement est filtré), même un bit dans un octet d'axe ressort."
        }
    };
    // Choix de la clé de déduplication selon le niveau :
    //   -v  : axes analogiques bruyants effacés (bouton clairement hors des axes) ;
    //   -vv : rien d'effacé, seul le quartet bas (tremblement) est masqué — révèle un
    //         bouton caché dans un octet d'axe, à condition de tenir les axes immobiles.
    let cle: fn(&[u8]) -> Vec<u8> = match verbose {
        0 => <[u8]>::to_vec,
        1 => cle_boutons,
        _ => cle_boutons_large,
    };
    boucle_lecture(en_tete, cle, move |rapport| {
        let entrees = entree::entrees_depuis_rapport(rapport);
        if verbose > 0 {
            format_entrees_diagnostic(rapport, entrees)
        } else {
            format_entrees(rapport, entrees)
        }
    })
}

/// Vide le descripteur de rapport HID de chaque interface (collection) du G27 natif.
///
/// Source de vérité du mapping HID : le descripteur dit, octet par octet, quels bits
/// sont des boutons, des axes, un chapeau, etc. — et le numéro que Windows leur donne.
/// Un G27 peut exposer **plusieurs collections** (entrées HID distinctes au même
/// VID/PID) ; un bouton invisible sur la collection qu'on lit peut vivre sur une autre.
/// Lecture seule (ouverture non exclusive) ; n'écrit rien au volant.
fn run_descripteur() -> ExitCode {
    let api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(erreur) => {
            eprintln!("Erreur : accès HID impossible : {erreur}");
            return ExitCode::FAILURE;
        }
    };
    let mut interfaces = 0u32;
    for info in hid::collect_logitech_devices(&api) {
        if info.mode != hid::G27Mode::Native {
            continue;
        }
        interfaces += 1;
        println!(
            "── Interface {} — usage_page {:#06x}, usage {:#06x} ──",
            info.interface_number, info.usage_page, info.usage
        );
        match api.open_path(info.path.as_c_str()) {
            Ok(device) => {
                let mut tampon = [0u8; 4096];
                match device.get_report_descriptor(&mut tampon) {
                    Ok(taille) => {
                        let hex: Vec<String> = tampon[..taille]
                            .iter()
                            .map(|octet| format!("{octet:02x}"))
                            .collect();
                        println!("descripteur ({taille} octets) :\n{}\n", hex.join(" "));
                    }
                    Err(erreur) => eprintln!("  descripteur indisponible : {erreur}\n"),
                }
            }
            Err(erreur) => eprintln!("  ouverture impossible : {erreur}\n"),
        }
    }
    if interfaces == 0 {
        eprintln!("Aucun G27 en mode natif détecté.");
        return ExitCode::FAILURE;
    }
    println!("{interfaces} interface(s) HID listée(s).");
    ExitCode::SUCCESS
}

/// Calibration de la boîte en H : relève la position X/Y du levier par vitesse.
///
/// Sans LGS, le G27 publie la position **analogique** du levier (octets X/Y) au lieu
/// de boutons de vitesse. Cet outil affiche cette position de façon stable — une
/// ligne par région — pour relever le couple (X, Y) de chaque vitesse et l'état
/// « enfoncé » (marche arrière). Ces relevés alimenteront la détection de vitesse.
fn run_levier() -> ExitCode {
    let en_tete = "Calibration de la boîte en H du G27 (Ctrl+C pour quitter).\n\
         Engagez et MAINTENEZ chaque vitesse (1 à 6) puis la marche arrière, quelques secondes.\n\
         Notez le couple (X, Y) et l'état « enfoncé » de chaque position, plus le point mort.";
    boucle_lecture(en_tete, cle_levier, format_levier)
}

/// Lit un octet du rapport (0 si hors limites).
fn octet_rapport(rapport: &[u8], index: usize) -> u8 {
    rapport.get(index).copied().unwrap_or(0)
}

/// Clé de déduplication du levier : quartet haut de X et de Y + bit « enfoncé ».
/// Stable quand le levier est tenu dans une vitesse → une ligne par position.
fn cle_levier(rapport: &[u8]) -> Vec<u8> {
    vec![
        octet_rapport(rapport, entree::OCTET_LEVIER_X) & 0xF0,
        octet_rapport(rapport, entree::OCTET_LEVIER_Y) & 0xF0,
        octet_rapport(rapport, entree::OCTET_LEVIER_ETAT) & entree::BIT_LEVIER_ENFONCE,
    ]
}

/// Formate la position du levier : rapport brut + X/Y (décimal et hexa) + enfoncé.
fn format_levier(rapport: &[u8]) -> String {
    let x = octet_rapport(rapport, entree::OCTET_LEVIER_X);
    let y = octet_rapport(rapport, entree::OCTET_LEVIER_Y);
    let enfonce =
        octet_rapport(rapport, entree::OCTET_LEVIER_ETAT) & entree::BIT_LEVIER_ENFONCE != 0;
    let hex: Vec<String> = rapport.iter().map(|octet| format!("{octet:02x}")).collect();
    format!(
        "[{hex}]  levier X={x:>3} (0x{x:02x})  Y={y:>3} (0x{y:02x})  enfoncé={enfonce_txt}",
        hex = hex.join(" "),
        enfonce_txt = if enfonce { "OUI" } else { "non" },
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

/// Variante diagnostic de [`format_entrees`] (mode `-v`) : sous la ligne décodée,
/// liste les bits armés du rapport au format `octet:bit`, en **séparant** les octets
/// de boutons (0–2, lus par le parsing) des octets d'axes (≥3). Un bit qui s'arme
/// dans la partie « axes » en pressant un bouton désigne exactement l'octet/bit que
/// le décodage ignore — c'est là que vivent les boutons manquants.
fn format_entrees_diagnostic(rapport: &[u8], entrees: EntreesG27) -> String {
    let base = format_entrees(rapport, entrees);
    let mut boutons = Vec::new();
    let mut axes = Vec::new();
    for (octet, valeur) in rapport.iter().enumerate() {
        for bit in 0u8..8 {
            if valeur & (1u8 << bit) != 0 {
                let cible = if octet < 3 { &mut boutons } else { &mut axes };
                cible.push(format!("{octet}:{bit}"));
            }
        }
    }
    format!(
        "{base}\n    bits boutons (octets 0-2) = [{}]  |  bits axes (octets ≥3) = [{}]",
        boutons.join(" "),
        axes.join(" "),
    )
}

/// Demande d'arrêt posée par le handler console (Ctrl+C / fermeture), lue par la
/// boucle d'attente.
static ARRET_DEMANDE: AtomicBool = AtomicBool::new(false);
/// Signale que le nettoyage (démasquage + libération vJoy) est terminé.
static NETTOYAGE_FINI: AtomicBool = AtomicBool::new(false);

/// Démarre le pont (feeder + masquage) et le maintient actif jusqu'à fermeture.
///
/// L'exécutable étant en sous-système GUI, PowerShell n'attend pas ce process et
/// Ctrl+C ne lui parvient pas toujours : **fermer la fenêtre console** déclenche
/// `CTRL_CLOSE`, que l'on intercepte pour garantir le démasquage + la libération
/// vJoy. Pour un contrôle confortable, préférez la GUI (bouton Démarrer/Arrêter).
fn run_feeder(id: Option<u32>, sans_masquage: bool) -> ExitCode {
    let config = config::Config::charger();
    // `--id` est prioritaire sur la config ; idem `--sans-masquage`.
    let id_vjoy = id.unwrap_or(config.pont.id_vjoy);
    let masquer = !sans_masquage && config.pont.masquer_g27_au_demarrage;

    let pont = match pont::Pont::demarrer(id_vjoy, masquer) {
        Ok(pont) => pont,
        Err(erreur) => {
            eprintln!("Erreur : {erreur}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Pont actif — device vJoy #{}, G27 {}.",
        pont.id_vjoy(),
        if pont.g27_masque() {
            "masqué au jeu"
        } else {
            "visible"
        }
    );
    println!(
        "Pour arrêter : FERMEZ cette fenêtre console (le G27 sera démasqué et le \
         device vJoy libéré). La GUI offre un arrêt plus confortable."
    );

    installer_handler_arret();
    while !ARRET_DEMANDE.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(150));
    }

    println!("Arrêt du pont…");
    drop(pont); // arrête le feeder (libère vJoy) PUIS démasque le G27 (ordre des champs).
    println!("Pont arrêté, G27 démasqué, device vJoy libéré.");
    // Débloque le handler console (cas CTRL_CLOSE, qui attend la fin du nettoyage).
    NETTOYAGE_FINI.store(true, Ordering::SeqCst);
    ExitCode::SUCCESS
}

/// Sous-commande `ffb` : aiguille vers l'action de diagnostic FFB.
fn run_ffb(action: FfbAction) -> ExitCode {
    match action {
        FfbAction::Capturer { id, fichier } => run_ffb_capturer(id, fichier),
        FfbAction::Pont { id, sans_masquage } => run_ffb_pont(id, sans_masquage),
        FfbAction::Force { valeur } => run_ffb_force(valeur),
    }
}

/// Démarre le pont FFB complet et le maintient jusqu'à fermeture de la console.
///
/// Le retour de force du jeu (via vJoy) est recopié vers le G27 à ~100 Hz ;
/// l'autocentrage matériel est coupé pendant le pont. À l'arrêt (fermeture console),
/// le `Drop` du `Pont` **garantit** la remise au neutre du volant + la restauration de
/// l'autocentrage, puis le démasquage et la libération vJoy.
fn run_ffb_pont(id: Option<u32>, sans_masquage: bool) -> ExitCode {
    let config = config::Config::charger();
    let id_vjoy = id.unwrap_or(config.pont.id_vjoy);
    let masquer = !sans_masquage && config.pont.masquer_g27_au_demarrage;
    let options = pont::OptionsPont {
        couper_autocentrage: config.pont.couper_autocentrage_ffb,
        chapeau_clavier: config.pont.chapeau_vers_clavier,
        bouton_valider: config.pont.bouton_valider,
        bouton_retour: config.pont.bouton_retour,
    };

    let pont = match pont::Pont::demarrer_pont_ffb(id_vjoy, masquer, options) {
        Ok(pont) => pont,
        Err(erreur) => {
            eprintln!("Erreur : {erreur}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Pont FFB actif — device vJoy #{} alimenté, G27 {}, autocentrage {}.",
        pont.id_vjoy(),
        if pont.g27_masque() {
            "masqué au jeu"
        } else {
            "visible"
        },
        if options.couper_autocentrage {
            "coupé"
        } else {
            "actif (résistance à l'arrêt)"
        }
    );
    if options.chapeau_clavier {
        println!("D-pad → flèches clavier activé (navigation menus/map).");
    }
    println!(
        "Lancez un jeu : le retour de force est recopié vers le volant. \
         ⚠️ Gardez la main sur le volant pour le premier essai."
    );
    println!("Pour arrêter (volant remis au neutre) : FERMEZ cette fenêtre console.");

    installer_handler_arret();
    while !ARRET_DEMANDE.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(150));
    }

    println!("Arrêt du pont FFB…");
    drop(pont); // stop des forces + autocentrage restauré, puis RelinquishVJD + démasquage.
    println!("Pont FFB arrêté, volant au neutre, G27 démasqué, device vJoy libéré.");
    NETTOYAGE_FINI.store(true, Ordering::SeqCst);
    ExitCode::SUCCESS
}

/// Capture les paquets FFB envoyés par le jeu au device vJoy et les affiche.
///
/// Démarre le **pont complet** (recopie G27 → vJoy + masquage, cycle de vie Phase 4)
/// et greffe le récepteur FFB sur le **même** device acquis : un jeu n'envoie du FFB
/// qu'à un volant vJoy actif (axes alimentés). L'acquisition + le callback vivent sur
/// le thread worker du feeder. Arrêt par fermeture de la console (`CTRL_CLOSE`) →
/// démasquage + `RelinquishVJD` garantis (Drop du pont).
fn run_ffb_capturer(id: Option<u32>, fichier: Option<String>) -> ExitCode {
    let config = config::Config::charger();
    let id_vjoy = id.unwrap_or(config.pont.id_vjoy);
    let masquer = config.pont.masquer_g27_au_demarrage;

    // Ouvre le fichier de sortie d'abord (échec rapide, avant d'acquérir vJoy).
    let mut sortie = match ouvrir_fichier_capture(fichier) {
        Ok(sortie) => sortie,
        Err(erreur) => {
            eprintln!("Erreur : {erreur}");
            return ExitCode::FAILURE;
        }
    };

    let (tx, paquets) = std::sync::mpsc::channel();
    let pont = match pont::Pont::demarrer_capture_ffb(id_vjoy, masquer, tx) {
        Ok(pont) => pont,
        Err(erreur) => {
            eprintln!("Erreur : {erreur}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "Capture FFB — device vJoy #{} alimenté, G27 {}.",
        pont.id_vjoy(),
        if pont.g27_masque() {
            "masqué au jeu"
        } else {
            "visible"
        }
    );
    println!(
        "Le device doit avoir « Enable Effects » activé (Configure vJoy) ; \
         envoyez des effets depuis un jeu."
    );
    if let Some((chemin, _)) = &sortie {
        println!("Enregistrement dans « {chemin} » (une ligne par paquet).");
    }
    println!("Pour arrêter : FERMEZ cette fenêtre console.");

    installer_handler_arret();
    let mut total = 0u64;
    while !ARRET_DEMANDE.load(Ordering::SeqCst) {
        while let Ok(message) = paquets.try_recv() {
            total += 1;
            ecrire_paquet_capture(&mut sortie, &message);
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    println!("Arrêt de la capture FFB… ({total} paquets reçus)");
    drop(pont); // arrête le feeder (libère vJoy) PUIS démasque le G27 (ordre des champs).
    println!("Capture FFB arrêtée, G27 démasqué, device vJoy libéré.");
    NETTOYAGE_FINI.store(true, Ordering::SeqCst);
    ExitCode::SUCCESS
}

/// Ouvre le fichier de capture si un chemin est fourni, en écrivant un en-tête.
/// Renvoie `Ok(None)` (sortie console seule) si aucun chemin n'est donné.
fn ouvrir_fichier_capture(
    chemin: Option<String>,
) -> Result<Option<(String, std::fs::File)>, std::io::Error> {
    let Some(chemin) = chemin else {
        return Ok(None);
    };
    let mut fichier = std::fs::File::create(&chemin)?;
    writeln!(
        fichier,
        "# Capture FFB G27 Mode Switcher — un paquet par ligne"
    )?;
    fichier.flush()?;
    Ok(Some((chemin, fichier)))
}

/// Écrit un paquet FFB capturé : sur la console toujours, et dans le fichier (vidé
/// immédiatement pour survivre à une fermeture brutale de la console) si demandé.
fn ecrire_paquet_capture(sortie: &mut Option<(String, std::fs::File)>, message: &ffb::MessageFfb) {
    println!("FFB reçu — {message:?}");
    if let Some((_, fichier)) = sortie
        && let Err(erreur) = writeln!(fichier, "{message:?}").and_then(|()| fichier.flush())
    {
        eprintln!("Attention : écriture du fichier de capture impossible : {erreur}");
    }
}

/// Garantit la **remise au neutre** du volant (stop des forces) à la sortie, quel
/// que soit le chemin : fin normale, Ctrl+C, fermeture de la console, erreur ou
/// panique. Même rigueur RAII que le démasquage `HidHide` de la Phase 4.
///
/// Le `Drop` ne panique jamais : un échec d'envoi est tracé et signalé, sans plus —
/// laisser remonter une panique depuis un `Drop` avorterait le process et pourrait
/// laisser une force résiduelle dans le firmware.
struct GardeForce {
    api: hidapi::HidApi,
    info: hid::DeviceInfo,
}

impl Drop for GardeForce {
    fn drop(&mut self) {
        let stop = ffb::commande_stop_forces();
        if let Err(erreur) = report::send_reports(
            &self.api,
            &self.info,
            std::slice::from_ref(&stop),
            Duration::ZERO,
        ) {
            tracing::error!("échec du stop FFB à la sortie : {erreur}");
            eprintln!("Attention : impossible de remettre le volant au neutre ({erreur}).");
        }
    }
}

/// Applique une **force constante** au G27 natif et la maintient jusqu'à fermeture.
///
/// ⚠️ Sécurité physique : commencez par de **très petites** valeurs. La force tient
/// tant que la console reste ouverte ; la remise au neutre (stop) est **garantie** à
/// la sortie par le `Drop` de [`GardeForce`] (créé avant le premier envoi).
fn run_ffb_force(valeur: i32) -> ExitCode {
    let api = match hidapi::HidApi::new() {
        Ok(api) => api,
        Err(erreur) => {
            eprintln!("Erreur : accès HID impossible : {erreur}");
            return ExitCode::FAILURE;
        }
    };
    let info = match hid::find_native_g27(&api) {
        Ok(info) => info,
        Err(hid::NativeLookup::NotNative) => {
            eprintln!(
                "Le G27 est en mode compatibilité : la force n'a aucun effet. Lancez « switch » d'abord."
            );
            return ExitCode::FAILURE;
        }
        Err(hid::NativeLookup::NoG27) => {
            eprintln!("Aucun G27 détecté. Branchez le volant puis réessayez.");
            return ExitCode::FAILURE;
        }
    };

    // Garde créé AVANT tout envoi de force : même une panique pendant l'envoi initial
    // déclenche alors le stop. À partir d'ici, tout retour passe par son `Drop`.
    let garde = GardeForce { api, info };

    let commande = ffb::commande_force_constante(valeur);
    if let Err(erreur) = report::send_reports(
        &garde.api,
        &garde.info,
        std::slice::from_ref(&commande),
        Duration::ZERO,
    ) {
        eprintln!("Échec de l'envoi de la force : {erreur}");
        return ExitCode::FAILURE; // le `Drop` du garde remet quand même au neutre.
    }

    println!(
        "Force constante appliquée au {} (couple {valeur}).",
        garde.info
    );
    println!("⚠️ Commencez toujours par de TRÈS PETITES valeurs et gardez la main sur le volant.");
    println!("Pour arrêter (et remettre le volant au neutre) : FERMEZ cette fenêtre console.");

    installer_handler_arret();
    while !ARRET_DEMANDE.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(150));
    }

    println!("Arrêt — remise du volant au neutre…");
    drop(garde); // envoie le stop (garanti).
    println!("Volant remis au neutre.");
    // Débloque le handler console (cas CTRL_CLOSE, qui attend la fin du nettoyage).
    NETTOYAGE_FINI.store(true, Ordering::SeqCst);
    ExitCode::SUCCESS
}

/// Installe le handler console qui déclenche l'arrêt propre du pont.
///
/// ⚠️ CONTRAT DE SÛRETÉ — le handler ne DOIT JAMAIS appeler `std::process::exit`
/// ni `abort` : cela court-circuiterait les `Drop` et laisserait le G27 masqué. Il
/// pose [`ARRET_DEMANDE`] (la boucle d'attente le lit et déclenche le `Drop` du
/// `Pont`), puis — pour `CTRL_CLOSE`/`LOGOFF`/`SHUTDOWN`, où le système termine le
/// process dès le retour du handler — **attend** que le nettoyage soit terminé.
#[allow(unsafe_code)]
fn installer_handler_arret() {
    #[cfg(windows)]
    {
        // SAFETY: enregistrement d'un handler console ; `handler_console` n'accède
        // qu'à des `static` atomiques.
        unsafe {
            windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(handler_console), 1);
        }
    }
}

/// Handler console : demande l'arrêt puis attend la fin du nettoyage (borné, pour
/// ne jamais dépasser le délai système ni bloquer indéfiniment).
#[cfg(windows)]
#[allow(unsafe_code)]
unsafe extern "system" fn handler_console(_type_evenement: u32) -> i32 {
    ARRET_DEMANDE.store(true, Ordering::SeqCst);
    let debut = std::time::Instant::now();
    while !NETTOYAGE_FINI.load(Ordering::SeqCst) && debut.elapsed() < Duration::from_millis(4500) {
        std::thread::sleep(Duration::from_millis(20));
    }
    1 // TRUE : événement géré.
}

/// Masque/démasque le G27 réel via `HidHide`, ou affiche sa disponibilité.
fn run_hidhide(action: HidhideAction) -> ExitCode {
    match action {
        HidhideAction::Statut => {
            if hidhide::disponible() {
                println!("HidHide est disponible et pilotable.");
            } else {
                println!("HidHide est indisponible.");
                println!("{}", hidhide::AIDE_HIDHIDE);
            }
            ExitCode::SUCCESS
        }
        HidhideAction::Actif { etat } => {
            let actif = matches!(etat, EtatActif::On);
            issue_hidhide(
                hidhide::definir_actif(actif),
                if actif {
                    "Masquage activé (SET_ACTIVE on)."
                } else {
                    "Masquage désactivé (SET_ACTIVE off)."
                },
            )
        }
        HidhideAction::Masquer => {
            let api = match hidapi::HidApi::new() {
                Ok(api) => api,
                Err(erreur) => {
                    eprintln!("Erreur : accès HID impossible : {erreur}");
                    return ExitCode::FAILURE;
                }
            };
            issue_hidhide(
                hidhide::masquer_g27(&api),
                "G27 masqué au jeu (cet exécutable reste autorisé à le lire).\n\
                 Pensez à « hidhide demasquer » ensuite — le masquage persiste sinon.\n\
                 En cas de blocage, l'app officielle « HidHide Configuration Client » peut tout réinitialiser.",
            )
        }
        HidhideAction::Demasquer => issue_hidhide(
            hidhide::demasquer(),
            "G27 démasqué (de nouveau visible de toutes les applications).",
        ),
    }
}

/// Rapporte l'issue d'une opération `HidHide` (succès, aide si indisponible).
fn issue_hidhide(resultat: Result<(), hidhide::ErreurHidHide>, succes: &str) -> ExitCode {
    match resultat {
        Ok(()) => {
            println!("{succes}");
            ExitCode::SUCCESS
        }
        Err(hidhide::ErreurHidHide::Indisponible) => {
            eprintln!("{}", hidhide::AIDE_HIDHIDE);
            ExitCode::FAILURE
        }
        Err(erreur) => {
            eprintln!("Erreur : {erreur}");
            ExitCode::FAILURE
        }
    }
}

/// Affiche le diagnostic des prérequis du pont (vJoy + `HidHide`).
fn run_pont(action: PontAction) -> ExitCode {
    match action {
        PontAction::Statut => {
            let prerequis = pont::detecter();
            afficher_composant("vJoy", &prerequis.vjoy);
            afficher_composant("HidHide", &prerequis.hidhide);
            println!();
            if prerequis.tout_disponible() {
                println!("Prérequis du pont : OK — le pont peut démarrer.");
            } else {
                println!("Prérequis manquants — le pont ne peut pas démarrer.");
            }
            ExitCode::SUCCESS
        }
    }
}

/// Affiche l'état d'un composant prérequis (disponible ou raison de l'absence).
fn afficher_composant(nom: &str, composant: &pont::Composant) {
    match composant.raison() {
        None => println!("\u{2713} {nom} : disponible"),
        Some(raison) => println!("\u{2717} {nom} : indisponible\n    {raison}"),
    }
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
