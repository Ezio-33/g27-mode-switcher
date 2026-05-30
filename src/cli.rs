//! Définition et exécution de l'interface en ligne de commande (clap).

use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand};

use crate::{hid, range, switcher};

/// Bascule un volant Logitech G27 vers son mode natif, sans pilote propriétaire.
#[derive(Debug, Parser)]
#[command(name = "g27-mode-switcher", version, about, long_about = None)]
pub struct Cli {
    /// Augmente la verbosité des logs (-v : debug, -vv : trace).
    #[arg(short, long, action = ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    command: Command,
}

/// Sous-commandes disponibles.
#[derive(Debug, Subcommand)]
enum Command {
    /// Liste les périphériques Logitech détectés.
    List,
    /// Bascule le G27 en mode natif (900°, retour de force complet).
    Switch {
        /// Simule l'opération : construit et valide le transfert sans rien envoyer.
        #[arg(long)]
        dry_run: bool,
    },
    /// Affiche le mode courant du G27 détecté.
    Status,
    /// Règle l'angle de rotation du G27 (mode natif requis), de 40° à 900°.
    SetRange {
        /// Angle de rotation souhaité, en degrés (40–900).
        degrees: u16,
    },
}

impl Cli {
    /// Exécute la sous-commande sélectionnée et renvoie le code de sortie.
    #[must_use]
    pub fn run(self) -> ExitCode {
        match self.command {
            Command::List => run_list(),
            Command::Switch { dry_run } => run_switch(dry_run),
            Command::Status => run_status(),
            Command::SetRange { degrees } => run_set_range(degrees),
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
fn run_switch(dry_run: bool) -> ExitCode {
    if dry_run {
        println!("Simulation (--dry-run) : aucune donnée ne sera envoyée au volant.");
    } else {
        println!("Bascule du G27 vers le mode natif (900°, retour de force complet)...");
        println!("Le volant va se déconnecter puis se reconnecter automatiquement.");
    }

    match switcher::switch_to_native_mode(dry_run) {
        Ok(outcome) if outcome.dry_run => {
            println!("Simulation OK : G27 éligible détecté → {}", outcome.device);
            ExitCode::SUCCESS
        }
        Ok(outcome) => {
            println!(
                "Magic packet envoyé au {}. Il va réapparaître en mode natif.",
                outcome.device
            );
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
