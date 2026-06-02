//! Pilotage du pilote HidHide via IOCTL sur son périphérique de contrôle.
//!
//! Module FFI : chaque bloc `unsafe` est documenté par un commentaire `// SAFETY:`.
#![allow(unsafe_code)]

use std::ffi::{OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use hidapi::HidApi;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

use super::ErreurHidHide;

/// Chemin du périphérique de contrôle HidHide.
const PERIPHERIQUE: &str = r"\\.\HidHide";

/// Type de périphérique IOCTL de HidHide.
const TYPE_HIDHIDE: u32 = 0x8001;
/// Méthode `METHOD_BUFFERED` (winioctl.h).
const METHOD_BUFFERED: u32 = 0;
/// Accès `FILE_READ_DATA` (winnt.h).
const FILE_READ_DATA: u32 = 0x0001;
/// Accès `FILE_WRITE_DATA` (winnt.h).
const FILE_WRITE_DATA: u32 = 0x0002;
/// Droit d'accès générique en lecture (winnt.h).
const GENERIC_READ: u32 = 0x8000_0000;
/// Droit d'accès générique en écriture (winnt.h).
const GENERIC_WRITE: u32 = 0x4000_0000;

/// Construit un code IOCTL (macro `CTL_CODE` de winioctl.h).
const fn ctl_code(type_peripherique: u32, fonction: u32, methode: u32, acces: u32) -> u32 {
    (type_peripherique << 16) | (acces << 14) | (fonction << 2) | methode
}

/// IOCTL « définir la liste blanche » (applications autorisées à voir le volant).
const IOCTL_SET_WHITELIST: u32 = ctl_code(
    TYPE_HIDHIDE,
    2049,
    METHOD_BUFFERED,
    FILE_READ_DATA | FILE_WRITE_DATA,
);
/// IOCTL « définir la liste noire » (périphériques à cacher).
const IOCTL_SET_BLACKLIST: u32 = ctl_code(
    TYPE_HIDHIDE,
    2051,
    METHOD_BUFFERED,
    FILE_READ_DATA | FILE_WRITE_DATA,
);
/// IOCTL « activer/désactiver le masquage ».
const IOCTL_SET_ACTIVE: u32 = ctl_code(
    TYPE_HIDHIDE,
    2053,
    METHOD_BUFFERED,
    FILE_READ_DATA | FILE_WRITE_DATA,
);

/// Indique si le périphérique de contrôle HidHide peut être ouvert.
pub(super) fn disponible() -> bool {
    Dispositif::ouvrir().is_ok()
}

/// Masque le G27 natif : liste blanche = notre exe, liste noire = le G27, actif.
pub(super) fn masquer_g27(api: &HidApi) -> Result<(), ErreurHidHide> {
    let exe = std::env::current_exe().map_err(|erreur| ErreurHidHide::Io(erreur.to_string()))?;
    let info = crate::hid::find_native_g27(api).map_err(|_| ErreurHidHide::G27Introuvable)?;
    let interface = info
        .path
        .to_str()
        .map_err(|_| ErreurHidHide::InstanceIllisible)?;
    let instance =
        super::instance_depuis_interface(interface).ok_or(ErreurHidHide::InstanceIllisible)?;

    let dispositif = Dispositif::ouvrir()?;
    dispositif.definir_liste(IOCTL_SET_WHITELIST, &[sans_nul(exe.as_os_str())])?;
    dispositif.definir_liste(IOCTL_SET_BLACKLIST, &[sans_nul(OsStr::new(&instance))])?;
    dispositif.definir_actif(true)
}

/// Désactive le masquage HidHide.
pub(super) fn demasquer() -> Result<(), ErreurHidHide> {
    let dispositif = Dispositif::ouvrir()?;
    dispositif.definir_actif(false)
}

/// Handle ouvert sur le périphérique de contrôle HidHide (fermé via `Drop`).
struct Dispositif(HANDLE);

impl Dispositif {
    fn ouvrir() -> Result<Self, ErreurHidHide> {
        let chemin = avec_nul(PERIPHERIQUE);
        // SAFETY: ouverture du périphérique de contrôle ; on valide le handle
        // renvoyé avant tout usage.
        let handle = unsafe {
            CreateFileW(
                chemin.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return Err(ErreurHidHide::Indisponible);
        }
        Ok(Self(handle))
    }

    fn definir_actif(&self, actif: bool) -> Result<(), ErreurHidHide> {
        self.ioctl(IOCTL_SET_ACTIVE, &[u8::from(actif)])
    }

    fn definir_liste(&self, code: u32, entrees: &[Vec<u16>]) -> Result<(), ErreurHidHide> {
        self.ioctl(code, &multi_sz(entrees))
    }

    fn ioctl(&self, code: u32, entree: &[u8]) -> Result<(), ErreurHidHide> {
        let taille = u32::try_from(entree.len())
            .map_err(|_| ErreurHidHide::Io("charge utile trop grande".to_owned()))?;
        let mut rendus = 0u32;
        // SAFETY: `entree` est un tampon valide de `taille` octets ; aucun tampon
        // de sortie (null/0) ; `rendus` reçoit le nombre d'octets renvoyés.
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                code,
                entree.as_ptr().cast::<c_void>(),
                taille,
                ptr::null_mut(),
                0,
                &raw mut rendus,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(ErreurHidHide::Io(format!(
                "DeviceIoControl a échoué (IOCTL {code:#x})"
            )));
        }
        Ok(())
    }
}

impl Drop for Dispositif {
    fn drop(&mut self) {
        // SAFETY: `self.0` est un handle valide ouvert par `ouvrir`.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// Encode une liste de chaînes UTF-16 (non terminées) en `REG_MULTI_SZ` (octets).
fn multi_sz(entrees: &[Vec<u16>]) -> Vec<u8> {
    let mut unites: Vec<u16> = Vec::new();
    for entree in entrees {
        unites.extend_from_slice(entree);
        unites.push(0);
    }
    unites.push(0); // terminateur final de la liste
    if unites.len() == 1 {
        unites.push(0); // liste vide → double nul
    }
    unites
        .iter()
        .flat_map(|unite| unite.to_le_bytes())
        .collect()
}

/// Convertit un `OsStr` en UTF-16 **sans** terminateur (séparateurs ajoutés par `multi_sz`).
fn sans_nul(valeur: &OsStr) -> Vec<u16> {
    valeur.encode_wide().collect()
}

/// Convertit une chaîne en UTF-16 **terminée par un nul** (pour `CreateFileW`).
fn avec_nul(valeur: &str) -> Vec<u16> {
    OsStr::new(valeur)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
