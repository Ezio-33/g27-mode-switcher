//! Lecture du dossier d'installation de vJoy dans le registre Windows.
//!
//! On parcourt les clés de désinstallation à la recherche d'une entrée dont le
//! `DisplayName` commence par « vJoy », puis on lit son `InstallLocation`.

use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
    RegQueryValueExW,
};

/// Clé contenant les entrées de désinstallation des logiciels installés.
const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
/// Taille (en `u16`) du tampon de nom de sous-clé.
const TAILLE_NOM: u32 = 256;

/// Renvoie le dossier d'installation de vJoy d'après le registre, si trouvé.
pub fn chemin_installation_vjoy() -> Option<PathBuf> {
    let uninstall = Cle::ouvrir(HKEY_LOCAL_MACHINE, UNINSTALL)?;
    for sous_cle in uninstall.sous_cles() {
        let Some(entree) = Cle::ouvrir(uninstall.0, &sous_cle) else {
            continue;
        };
        let nom = entree.valeur_texte("DisplayName").unwrap_or_default();
        if !nom.starts_with("vJoy") {
            continue;
        }
        if let Some(emplacement) = entree.valeur_texte("InstallLocation") {
            let emplacement = emplacement.trim().trim_end_matches(['\\', '/']);
            if !emplacement.is_empty() {
                return Some(PathBuf::from(emplacement));
            }
        }
    }
    None
}

/// Clé de registre ouverte, fermée automatiquement à la destruction.
struct Cle(HKEY);

impl Cle {
    /// Ouvre une sous-clé en lecture seule.
    fn ouvrir(parent: HKEY, sous_cle: &str) -> Option<Self> {
        let sous_cle = vers_wide(sous_cle);
        let mut handle: HKEY = ptr::null_mut();
        // SAFETY: ouverture en lecture ; `handle` reçoit une clé valide ssi le
        // code renvoyé est `ERROR_SUCCESS`.
        let code =
            unsafe { RegOpenKeyExW(parent, sous_cle.as_ptr(), 0, KEY_READ, &raw mut handle) };
        (code == ERROR_SUCCESS).then_some(Self(handle))
    }

    /// Énumère les noms des sous-clés.
    fn sous_cles(&self) -> Vec<String> {
        let mut noms = Vec::new();
        let mut index = 0u32;
        loop {
            let mut tampon = [0u16; TAILLE_NOM as usize];
            let mut longueur = TAILLE_NOM;
            // SAFETY: `tampon`/`longueur` décrivent un tampon valide ; les autres
            // paramètres optionnels sont nuls.
            let code = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    tampon.as_mut_ptr(),
                    &raw mut longueur,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            if code != ERROR_SUCCESS {
                break;
            }
            noms.push(String::from_utf16_lossy(&tampon[..longueur as usize]));
            index += 1;
        }
        noms
    }

    /// Lit une valeur `REG_SZ` (chaîne) de la clé.
    fn valeur_texte(&self, nom: &str) -> Option<String> {
        let nom = vers_wide(nom);
        let mut type_valeur = 0u32;
        let mut taille = 0u32;
        // SAFETY: 1er appel avec `lpData` nul → renvoie la taille nécessaire dans
        // `taille` et le type dans `type_valeur`.
        let code = unsafe {
            RegQueryValueExW(
                self.0,
                nom.as_ptr(),
                ptr::null(),
                &raw mut type_valeur,
                ptr::null_mut(),
                &raw mut taille,
            )
        };
        if code != ERROR_SUCCESS || type_valeur != REG_SZ || taille == 0 {
            return None;
        }
        let mut donnees = vec![0u8; taille as usize];
        let mut taille_lue = taille;
        // SAFETY: `donnees` est dimensionné selon la taille renvoyée au 1er appel.
        let code = unsafe {
            RegQueryValueExW(
                self.0,
                nom.as_ptr(),
                ptr::null(),
                &raw mut type_valeur,
                donnees.as_mut_ptr(),
                &raw mut taille_lue,
            )
        };
        if code != ERROR_SUCCESS {
            return None;
        }
        let unites: Vec<u16> = donnees[..taille_lue as usize]
            .chunks_exact(2)
            .map(|paire| u16::from_ne_bytes([paire[0], paire[1]]))
            .collect();
        Some(
            String::from_utf16_lossy(&unites)
                .trim_end_matches('\0')
                .to_owned(),
        )
    }
}

impl Drop for Cle {
    fn drop(&mut self) {
        // SAFETY: `self.0` est une clé valide ouverte par `ouvrir`.
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

/// Convertit une chaîne en tampon UTF-16 terminé par un nul (pour l'API Win32).
fn vers_wide(valeur: &str) -> Vec<u16> {
    std::ffi::OsStr::new(valeur)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
