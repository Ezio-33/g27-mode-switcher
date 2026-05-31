//! Définition et exécution de l'interface en ligne de commande (clap).

use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand, ValueEnum};

use g27_mode_switcher::{autocenter, hid, range, switcher};

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
#[derive(Debug, Clone, Copy, Subcommand)]
enum Command {
    /// Liste les périphériques Logitech détectés.
    List,
    /// Bascule le G27 en mode natif (900°, retour de force complet).
    Switch {
        /// Simule l'opération : construit et valide le transfert sans rien envoyer.
        #[arg(long)]
        dry_run: bool,
        /// Ne règle pas automatiquement l'angle à 900° après la bascule.
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
}

/// État cible de l'autocentrage matériel.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum AutocenterState {
    /// Désactive l'autocentrage matériel.
    Off,
    /// Réactive l'autocentrage (réglage paramétrable prévu en v0.3.0).
    On,
}

impl Cli {
    /// Exécute la sous-commande choisie, ou lance la GUI si aucune n'est fournie.
    #[must_use]
    pub fn run(self) -> ExitCode {
        match self.command {
            Some(command) => dispatch(command),
            None => run_gui(),
        }
    }
}

/// Exécute une sous-commande CLI et renvoie son code de sortie.
fn dispatch(command: Command) -> ExitCode {
    match command {
        Command::List => run_list(),
        Command::Switch {
            dry_run,
            no_range,
            disable_autocenter,
        } => run_switch(dry_run, no_range, disable_autocenter),
        Command::Status => run_status(),
        Command::SetRange { degrees } => run_set_range(degrees),
        Command::SetAutocenter { state } => run_set_autocenter(state),
    }
}

/// Lance l'interface graphique (mode par défaut).
fn run_gui() -> ExitCode {
    match crate::gui::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Erreur de l'interface graphique : {error}");
            ExitCode::FAILURE
        }
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
fn run_switch(dry_run: bool, no_range: bool, disable_autocenter: bool) -> ExitCode {
    if dry_run {
        println!("Simulation (--dry-run) : aucune donnée ne sera envoyée au volant.");
    } else {
        println!("Bascule du G27 vers le mode natif (900°, retour de force complet)...");
        println!("Le volant va se déconnecter puis se reconnecter automatiquement.");
    }

    match switcher::switch_to_native_mode(dry_run, !no_range, disable_autocenter) {
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

/// Active/désactive l'autocentrage matériel (seule la désactivation est gérée).
fn run_set_autocenter(state: AutocenterState) -> ExitCode {
    if matches!(state, AutocenterState::On) {
        eprintln!(
            "La réactivation paramétrable de l'autocentrage arrivera en v0.3.0. (L'autocentrage se réactive de toute façon au rebranchement du volant.)"
        );
        return ExitCode::FAILURE;
    }

    match autocenter::disable_autocenter() {
        Ok(outcome) => {
            println!(
                "Autocentrage matériel désactivé pour le {outcome}. Sans couche FFB active, le volant n'aura plus de force de centrage.",
                outcome = outcome.device
            );
            ExitCode::SUCCESS
        }
        Err(autocenter::Error::NotNative) => {
            eprintln!(
                "Le G27 est en mode compatibilité : la désactivation n'a aucun effet. Lancez « switch » d'abord."
            );
            ExitCode::FAILURE
        }
        Err(autocenter::Error::NoG27Found) => {
            eprintln!("Aucun G27 détecté. Branchez le volant puis réessayez.");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("Échec de la désactivation de l'autocentrage : {error}");
            ExitCode::FAILURE
        }
    }
}
