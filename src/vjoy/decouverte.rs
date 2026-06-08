//! Recherche automatique de `vJoyInterface.dll` (x64).
//!
//! L'utilisateur final ne doit jamais copier la DLL à la main : on la cherche
//! dans l'ordre (1) à côté de l'exécutable, (2) à l'emplacement standard
//! `%ProgramFiles%\vJoy\x64`, (3) via le chemin d'installation lu dans le
//! registre Windows. Le chargement se fait ensuite sur le **chemin complet**.

use std::path::PathBuf;

/// Nom du fichier de la DLL d'interface vJoy (build x64).
const NOM_DLL: &str = "vJoyInterface.dll";

/// Résultat de la recherche de la DLL vJoy.
pub struct Recherche {
    /// Chemin trouvé, le cas échéant.
    pub trouve: Option<PathBuf>,
    /// Tous les chemins testés, dans l'ordre (pour un message d'erreur clair).
    pub testes: Vec<PathBuf>,
}

/// Cherche `vJoyInterface.dll` selon l'ordre de priorité décrit dans le module.
#[must_use]
pub fn rechercher_dll() -> Recherche {
    let mut testes = Vec::new();
    for candidat in chemins_candidats() {
        let existe = candidat.is_file();
        testes.push(candidat.clone());
        if existe {
            return Recherche {
                trouve: Some(candidat),
                testes,
            };
        }
    }
    Recherche {
        trouve: None,
        testes,
    }
}

/// Construit la liste ordonnée des emplacements candidats.
fn chemins_candidats() -> Vec<PathBuf> {
    let mut chemins = Vec::new();

    // 1. À côté de l'exécutable (cas où la DLL a été copiée manuellement).
    if let Ok(exe) = std::env::current_exe()
        && let Some(dossier) = exe.parent()
    {
        chemins.push(dossier.join(NOM_DLL));
    }

    // 2 & 3 : emplacements Windows (variable d'environnement + registre).
    #[cfg(windows)]
    {
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            chemins.push(
                PathBuf::from(program_files)
                    .join("vJoy")
                    .join("x64")
                    .join(NOM_DLL),
            );
        }
        if let Some(install) = super::registre::chemin_installation_vjoy() {
            chemins.push(install.join("x64").join(NOM_DLL));
        }
    }

    chemins
}
