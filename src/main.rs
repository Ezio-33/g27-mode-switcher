//! Point d'entrée du G27 Mode Switcher.
//!
//! Le dispatch d'arguments ci-dessous est volontairement minimal et
//! temporaire : il sera remplacé par une vraie CLI `clap` (sous-commandes
//! `list` / `switch` / `status`, options `--verbose` / `--dry-run`) à l'étape 4.

mod switcher;
mod usb;

use std::process::ExitCode;

use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    init_logging();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("list") => run_list(),
        Some("switch") => run_switch(args.iter().any(|arg| arg == "--dry-run")),
        Some(other) => {
            eprintln!("Commande inconnue : « {other} ». Commandes : list, switch [--dry-run].");
            ExitCode::FAILURE
        }
    }
}

/// Liste les périphériques Logitech détectés.
fn run_list() -> ExitCode {
    match usb::list_logitech_devices() {
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

/// Initialise le logging structuré, filtrable via la variable `RUST_LOG`
/// (niveau `info` par défaut).
fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
