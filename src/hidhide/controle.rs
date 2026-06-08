//! Pilotage du pilote HidHide via IOCTL sur son périphérique de contrôle.
//!
//! Module FFI : chaque bloc `unsafe` est documenté par un commentaire `// SAFETY:`.
#![allow(unsafe_code)]

use std::ffi::{OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::ptr;

use hidapi::HidApi;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;

use super::ErreurHidHide;

/// Chemin du périphérique de contrôle HidHide.
const PERIPHERIQUE: &str = r"\\.\HidHide";

/// Type de périphérique IOCTL de HidHide (`IoControlDeviceType` du contrat partagé
/// `Shared/HidHideIoctlContract.h`).
const TYPE_HIDHIDE: u32 = 32769;
/// Méthode `METHOD_BUFFERED` (winioctl.h).
const METHOD_BUFFERED: u32 = 0;
/// Accès `FILE_READ_DATA` (winnt.h). Le contrat HidHide utilise **cet accès pour
/// tous les IOCTL**, y compris les `SET_*` (et non `READ | WRITE`) : c'était le bug
/// — un accès à 3 changeait le code IOCTL, que le pilote rejetait alors avec
/// `ERROR_INVALID_PARAMETER` (87).
const FILE_READ_DATA: u32 = 0x0001;
/// Droit d'accès générique en lecture (winnt.h).
const GENERIC_READ: u32 = 0x8000_0000;

/// Construit un code IOCTL (macro `CTL_CODE` de winioctl.h).
const fn ctl_code(type_peripherique: u32, fonction: u32, methode: u32, acces: u32) -> u32 {
    (type_peripherique << 16) | (acces << 14) | (fonction << 2) | methode
}

/// IOCTL « définir la liste blanche » (0x8001_6004).
const IOCTL_SET_WHITELIST: u32 = ctl_code(TYPE_HIDHIDE, 2049, METHOD_BUFFERED, FILE_READ_DATA);
/// IOCTL « définir la liste noire » (0x8001_600C).
const IOCTL_SET_BLACKLIST: u32 = ctl_code(TYPE_HIDHIDE, 2051, METHOD_BUFFERED, FILE_READ_DATA);
/// IOCTL « activer/désactiver le masquage » (0x8001_6014).
const IOCTL_SET_ACTIVE: u32 = ctl_code(TYPE_HIDHIDE, 2053, METHOD_BUFFERED, FILE_READ_DATA);

// TODO (cycle de vie du masquage, amélioration future) : le contrat expose aussi un
// masquage DE SESSION non persistant — ADD_SESSION_BLACKLIST (2056) /
// CLR_SESSION_BLACKLIST (2057) — plus sûr que la liste noire permanente (qui laisse
// le G27 caché après l'arrêt de l'app). Le client officiel ne les utilise pas ;
// leur format de buffer reste à confirmer.

/// Indique si le périphérique de contrôle HidHide peut être ouvert.
pub(super) fn disponible() -> bool {
    Dispositif::ouvrir().is_ok()
}

/// Active (`true`) ou désactive (`false`) le masquage, sans toucher aux listes.
///
/// Permet de tester `SET_ACTIVE` en isolation (validation des codes IOCTL).
pub(super) fn definir_actif(actif: bool) -> Result<(), ErreurHidHide> {
    Dispositif::ouvrir()?.definir_actif(actif)
}

/// Masque le G27 : liste blanche = notre exe (chemin volume), liste noire =
/// toutes les interfaces du volant, puis active le masquage.
pub(super) fn masquer_g27(api: &HidApi) -> Result<(), ErreurHidHide> {
    let exe = std::env::current_exe().map_err(|erreur| ErreurHidHide::Io(erreur.to_string()))?;
    // Liste blanche : notre exe au format volume attendu par HidHide, sinon le
    // feeder perdrait lui-même l'accès au G27 une fois le masquage actif.
    let exe_volume = super::volume::chemin_volume(&exe).ok_or_else(|| {
        ErreurHidHide::Io("conversion du chemin volume de l'exe impossible".to_owned())
    })?;

    // Liste noire : toutes les interfaces HID du volant (le jeu ne doit en voir
    // aucune).
    let instances = super::instances_g27(api);
    if instances.is_empty() {
        return Err(ErreurHidHide::G27Introuvable);
    }
    let noires: Vec<Vec<u16>> = instances
        .iter()
        .map(|instance| sans_nul(OsStr::new(instance)))
        .collect();

    let dispositif = Dispositif::ouvrir()?;
    dispositif.definir_liste(IOCTL_SET_WHITELIST, &[sans_nul(OsStr::new(&exe_volume))])?;
    dispositif.definir_liste(IOCTL_SET_BLACKLIST, &noires)?;
    dispositif.definir_actif(true)
}

/// Désactive le masquage puis vide la liste noire (rien ne reste caché).
pub(super) fn demasquer() -> Result<(), ErreurHidHide> {
    let dispositif = Dispositif::ouvrir()?;
    dispositif.definir_actif(false)?;
    dispositif.definir_liste(IOCTL_SET_BLACKLIST, &[])
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
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
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
            // GetLastError doit être lu immédiatement après l'appel échoué.
            // SAFETY: appel sans paramètre, juste après l'échec de DeviceIoControl.
            let erreur = unsafe { GetLastError() };
            return Err(ErreurHidHide::Io(format!(
                "DeviceIoControl a échoué (IOCTL {code:#x}, {})",
                libelle_erreur(erreur)
            )));
        }
        tracing::debug!("HidHide IOCTL {code:#x} ({} octets) = OK", entree.len());
        Ok(())
    }
}

/// Traduit un code `GetLastError` Win32 fréquent en libellé lisible.
fn libelle_erreur(code: u32) -> String {
    let nom = match code {
        1 => "ERROR_INVALID_FUNCTION (code IOCTL non reconnu par le pilote)",
        5 => "ERROR_ACCESS_DENIED",
        6 => "ERROR_INVALID_HANDLE",
        50 => "ERROR_NOT_SUPPORTED",
        87 => "ERROR_INVALID_PARAMETER (format de buffer)",
        122 => "ERROR_INSUFFICIENT_BUFFER",
        _ => "voir code Win32",
    };
    format!("GetLastError {code} = {nom}")
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
    // Format identique à `Utils.cpp::StringListToMultiString` : chaque chaîne
    // suivie d'un nul, puis un nul final (liste vide → un seul nul).
    for entree in entrees {
        unites.extend_from_slice(entree);
        unites.push(0);
    }
    unites.push(0);
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
