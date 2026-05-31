// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Samuel.V (Ezio_33) — https://la-confrerie-des-ombres.vercel.app

// Sous-système « windows » : aucune console noire au lancement de la GUI. En
// mode CLI (lancement depuis un terminal), on rattache la console parente au
// démarrage pour que la sortie reste visible.
#![windows_subsystem = "windows"]

//! Point d'entrée du G27 Mode Switcher (CLI + interface graphique).

mod cli;
mod gui;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;

fn main() -> ExitCode {
    attach_parent_console();
    Cli::parse().run()
}

/// Rattache la console du processus parent si le binaire a été lancé depuis un
/// terminal, afin que la sortie CLI reste visible malgré le sous-système
/// « windows ». Sans console parente (double-clic), l'appel échoue silencieusement
/// et la GUI se lance proprement, sans fenêtre console.
#[cfg(windows)]
#[allow(unsafe_code)]
fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

    // SAFETY: appel FFI Win32 sans paramètre mémoire ; le code de retour est
    // volontairement ignoré (un échec signifie simplement « pas de console
    // parente », cas du double-clic).
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

/// Hors Windows, il n'y a pas de console à rattacher.
#[cfg(not(windows))]
fn attach_parent_console() {}
