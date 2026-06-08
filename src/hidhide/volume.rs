//! Conversion d'un chemin DOS en chemin « volume » attendu par HidHide.
//!
//! HidHide stocke la liste blanche en chemins noyau
//! (`\Device\HarddiskVolumeX\…`) et non en chemins DOS (`C:\…`). On reproduit la
//! logique de `Volume.cpp::FileNameToFullImageName` du client officiel :
//! point de montage → nom de volume → nom de périphérique DOS, puis on recolle
//! la partie du chemin située après le point de montage.
#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Storage::FileSystem::{
    GetVolumeNameForVolumeMountPointW, GetVolumePathNameW, QueryDosDeviceW,
};

/// Taille des tampons d'API (en `u16`).
const TAILLE_TAMPON: u32 = 4096;

/// Convertit `chemin` (ex. `C:\Users\…\app.exe`) au format volume HidHide
/// (ex. `\Device\HarddiskVolume3\Users\…\app.exe`). `None` si une étape échoue.
pub(super) fn chemin_volume(chemin: &Path) -> Option<String> {
    let chemin_str = chemin.to_str()?;
    let chemin_wide = avec_nul(chemin.as_os_str());

    // 1. Point de montage (ex. "C:\").
    let mut tampon = [0u16; TAILLE_TAMPON as usize];
    // SAFETY: tampon valide de TAILLE_TAMPON unités ; entrée terminée par un nul.
    let ok =
        unsafe { GetVolumePathNameW(chemin_wide.as_ptr(), tampon.as_mut_ptr(), TAILLE_TAMPON) };
    if ok == 0 {
        return None;
    }
    let point_montage = depuis_wide(&tampon);

    // 2. Nom de volume (ex. "\\?\Volume{guid}\").
    let montage_wide = avec_nul(OsStr::new(&point_montage));
    let mut tampon = [0u16; TAILLE_TAMPON as usize];
    // SAFETY: idem.
    let ok = unsafe {
        GetVolumeNameForVolumeMountPointW(montage_wide.as_ptr(), tampon.as_mut_ptr(), TAILLE_TAMPON)
    };
    if ok == 0 {
        return None;
    }
    let nom_volume = depuis_wide(&tampon);
    if nom_volume.len() < 6 || !nom_volume.is_char_boundary(nom_volume.len() - 1) {
        return None;
    }

    // 3. Nom de périphérique DOS : QueryDosDeviceW("Volume{guid}") — on retire le
    //    préfixe "\\?\" (4 caractères) et le "\" final (cf. Volume.cpp).
    let cle_volume = &nom_volume[4..nom_volume.len() - 1];
    let cle_wide = avec_nul(OsStr::new(cle_volume));
    let mut tampon = [0u16; TAILLE_TAMPON as usize];
    // SAFETY: idem ; QueryDosDeviceW renvoie le nombre d'unités écrites (0 = échec).
    let n = unsafe { QueryDosDeviceW(cle_wide.as_ptr(), tampon.as_mut_ptr(), TAILLE_TAMPON) };
    if n == 0 {
        return None;
    }
    let dos = depuis_wide(&tampon);

    // 4. dosDevice + (chemin privé du point de montage).
    let reste = chemin_str.get(point_montage.len()..)?;
    Some(format!("{dos}\\{reste}"))
}

/// Tampon `OsStr` → UTF-16 terminé par un nul (pour les paramètres d'entrée).
fn avec_nul(valeur: &OsStr) -> Vec<u16> {
    valeur.encode_wide().chain(std::iter::once(0)).collect()
}

/// Lit une chaîne UTF-16 terminée par un nul depuis un tampon.
fn depuis_wide(tampon: &[u16]) -> String {
    let fin = tampon
        .iter()
        .position(|&unite| unite == 0)
        .unwrap_or(tampon.len());
    String::from_utf16_lossy(&tampon[..fin])
}
