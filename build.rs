//! Script de build : embarque l'icône applicative dans l'exécutable Windows.
//!
//! Pour les cibles Windows uniquement, on compile un mini-fichier de ressources
//! (`.rc`) référençant `assets/icon/icon.ico` via **windres** (fourni par mingw-w64,
//! déjà requis pour la cross-compilation) et on lie l'objet COFF résultant au binaire.
//! Aucune dépendance crate n'est ajoutée. Sur les autres cibles (Linux/macOS) ou si
//! windres est introuvable, le script ne fait rien — le build reste fonctionnel.

use std::path::Path;
use std::process::Command;

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }

    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let icone = Path::new(&manifest).join("assets/icon/icon.ico");
    if !icone.exists() {
        return;
    }
    println!("cargo:rerun-if-changed=assets/icon/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = std::env::var("OUT_DIR").unwrap_or_default();
    let rc = Path::new(&out_dir).join("app_icon.rc");
    let res = Path::new(&out_dir).join("app_icon.res");

    // `.rc` minimal : icône applicative d'ID 1 (Explorer affiche l'icône d'ID le plus bas).
    let chemin_icone = icone.display().to_string().replace('\\', "/");
    if std::fs::write(&rc, format!("1 ICON \"{chemin_icone}\"\n")).is_err() {
        return;
    }

    // windres préfixé selon la cible (cross mingw), avec repli sur `windres`.
    for windres in [
        format!("{}-windres", prefixe_mingw(&target)),
        "windres".into(),
    ] {
        if windres.starts_with('-') {
            continue;
        }
        let statut = Command::new(&windres)
            .arg("-O")
            .arg("coff")
            .arg("-i")
            .arg(&rc)
            .arg("-o")
            .arg(&res)
            .status();
        if matches!(statut, Ok(code) if code.success()) {
            println!("cargo:rustc-link-arg-bins={}", res.display());
            return;
        }
    }
    println!("cargo:warning=icône Windows non embarquée (windres introuvable ou en échec)");
}

/// Préfixe de la toolchain mingw correspondant à la cible (`x86_64-w64-mingw32`, …).
fn prefixe_mingw(target: &str) -> &'static str {
    if target.starts_with("x86_64") {
        "x86_64-w64-mingw32"
    } else if target.starts_with("i686") {
        "i686-w64-mingw32"
    } else if target.starts_with("aarch64") {
        "aarch64-w64-mingw32"
    } else {
        ""
    }
}
