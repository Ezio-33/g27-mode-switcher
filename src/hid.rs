//! Détection des périphériques HID Logitech et identification du mode du G27.
//!
//! Ce module énumère les périphériques via l'API HID native (`hidapi`) :
//! `hidraw` sous Linux, `HidUsb`/`setupapi` sous Windows. Il ne dépossède
//! jamais le pilote HID du système (contrairement à une approche USB raw type
//! `WinUSB`) et ne requiert donc ni Zadig ni privilèges élevés. La bascule de
//! mode proprement dite relève du module `switcher`.

use std::ffi::CString;
use std::fmt;

/// Vendor ID Logitech.
///
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c` (`USB_VENDOR_ID_LOGITECH`).
pub const LOGITECH_VENDOR_ID: u16 = 0x046D;

/// Product ID du G27 en mode compatibilité « Driving Force EX » (au branchement).
///
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`.
pub const G27_COMPAT_PRODUCT_ID: u16 = 0xC294;

/// Product ID du G27 en mode natif, après la bascule.
///
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`.
pub const G27_NATIVE_PRODUCT_ID: u16 = 0xC29B;

/// Mode d'un périphérique Logitech vis-à-vis du G27.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum G27Mode {
    /// Mode compatibilité (200°, retour de force bridé) — PID `0xC294`.
    Compatibility,
    /// Mode natif G27 (900°, retour de force complet) — PID `0xC29B`.
    Native,
    /// Autre périphérique Logitech (non G27 ou inconnu).
    Other,
}

/// Classe un Product ID Logitech selon le mode G27 correspondant.
#[must_use]
pub fn classify_product_id(product_id: u16) -> G27Mode {
    match product_id {
        G27_COMPAT_PRODUCT_ID => G27Mode::Compatibility,
        G27_NATIVE_PRODUCT_ID => G27Mode::Native,
        _ => G27Mode::Other,
    }
}

/// Informations sur un périphérique HID Logitech détecté.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Vendor ID (toujours [`LOGITECH_VENDOR_ID`] ici).
    pub vendor_id: u16,
    /// Product ID brut tel qu'annoncé par le périphérique.
    pub product_id: u16,
    /// Numéro d'interface HID (`-1` si non applicable selon la plateforme).
    pub interface_number: i32,
    /// `Usage Page` de la collection HID de haut niveau (`0` si non renseigné,
    /// fréquent sous Linux hidraw). Utile pour distinguer plusieurs collections
    /// d'un même périphérique sous Windows.
    pub usage_page: u16,
    /// `Usage` de la collection HID de haut niveau (`0` si non renseigné).
    pub usage: u16,
    /// Chemin système opaque du périphérique HID, utilisé pour l'ouvrir
    /// précisément (identifiant stable durant la session).
    pub path: CString,
    /// Mode déduit du Product ID.
    pub mode: G27Mode,
}

impl DeviceInfo {
    /// Indique si le périphérique est un G27 (quel que soit son mode).
    #[must_use]
    pub fn is_g27(&self) -> bool {
        matches!(self.mode, G27Mode::Compatibility | G27Mode::Native)
    }
}

impl fmt::Display for DeviceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self.mode {
            G27Mode::Compatibility => "G27 (mode compatibilité)",
            G27Mode::Native => "G27 (mode natif)",
            G27Mode::Other => "périphérique Logitech",
        };
        write!(
            f,
            "{label} — VID {:#06x} PID {:#06x} (interface {})",
            self.vendor_id, self.product_id, self.interface_number
        )
    }
}

/// Erreurs du module HID.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Échec d'accès au sous-système HID (initialisation hidapi, énumération…).
    #[error("failed to access the HID subsystem: {0}")]
    Access(#[from] hidapi::HidError),
}

/// Convertit une entrée d'énumération hidapi en [`DeviceInfo`].
pub(crate) fn device_info_from(entry: &hidapi::DeviceInfo) -> DeviceInfo {
    let product_id = entry.product_id();
    DeviceInfo {
        vendor_id: entry.vendor_id(),
        product_id,
        interface_number: entry.interface_number(),
        usage_page: entry.usage_page(),
        usage: entry.usage(),
        path: entry.path().to_owned(),
        mode: classify_product_id(product_id),
    }
}

/// Énumère les périphériques HID Logitech actuellement connectés.
///
/// L'opération lit uniquement les descripteurs d'énumération ; elle n'ouvre
/// aucun périphérique et ne requiert donc pas de privilèges élevés.
///
/// # Errors
///
/// Renvoie [`Error::Access`] si le sous-système HID ne peut pas être interrogé
/// (hidapi indisponible, énumération impossible).
pub fn list_logitech_devices() -> Result<Vec<DeviceInfo>, Error> {
    let api = hidapi::HidApi::new()?;
    Ok(collect_logitech_devices(&api))
}

/// Filtre les périphériques Logitech d'une instance hidapi déjà initialisée.
///
/// Factorisé pour permettre au module `switcher` de réutiliser la même
/// instance [`hidapi::HidApi`] que celle servant ensuite à l'ouverture.
#[must_use]
pub fn collect_logitech_devices(api: &hidapi::HidApi) -> Vec<DeviceInfo> {
    let mut devices = Vec::new();
    for entry in api.device_list() {
        if entry.vendor_id() != LOGITECH_VENDOR_ID {
            continue;
        }
        let info = device_info_from(entry);
        tracing::debug!(
            mode = ?info.mode,
            "périphérique HID Logitech détecté : VID {:#06x}, PID {:#06x}, interface {}",
            info.vendor_id,
            info.product_id,
            info.interface_number
        );
        devices.push(info);
    }
    devices
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceInfo, G27_COMPAT_PRODUCT_ID, G27_NATIVE_PRODUCT_ID, G27Mode, LOGITECH_VENDOR_ID,
        classify_product_id,
    };
    use std::ffi::CString;

    fn device(product_id: u16) -> DeviceInfo {
        DeviceInfo {
            vendor_id: LOGITECH_VENDOR_ID,
            product_id,
            interface_number: 0,
            usage_page: 0x0001,
            usage: 0x0004,
            path: CString::new("test-path").expect("chemin de test valide"),
            mode: classify_product_id(product_id),
        }
    }

    #[test]
    fn classifies_compatibility_pid() {
        assert_eq!(
            classify_product_id(G27_COMPAT_PRODUCT_ID),
            G27Mode::Compatibility
        );
    }

    #[test]
    fn classifies_native_pid() {
        assert_eq!(classify_product_id(G27_NATIVE_PRODUCT_ID), G27Mode::Native);
    }

    #[test]
    fn classifies_unknown_pid_as_other() {
        assert_eq!(classify_product_id(0x0000), G27Mode::Other);
        assert_eq!(classify_product_id(0xFFFF), G27Mode::Other);
    }

    #[test]
    fn recognizes_g27_modes() {
        assert!(device(G27_COMPAT_PRODUCT_ID).is_g27());
        assert!(device(G27_NATIVE_PRODUCT_ID).is_g27());
        assert!(!device(0x1234).is_g27());
    }

    #[test]
    fn display_includes_pid_and_mode() {
        let rendered = device(G27_NATIVE_PRODUCT_ID).to_string();
        assert!(rendered.contains("0xc29b"), "rendu: {rendered}");
        assert!(rendered.contains("natif"), "rendu: {rendered}");
    }

    #[test]
    fn display_labels_compatibility_and_other() {
        assert!(
            device(G27_COMPAT_PRODUCT_ID)
                .to_string()
                .contains("compatibilité")
        );
        assert!(device(0x1234).to_string().contains("périphérique Logitech"));
    }
}

/// Tests d'intégration nécessitant un G27 réellement branché.
/// Activés via la feature `hardware-tests` (voir tests/README.md).
#[cfg(all(test, feature = "hardware-tests"))]
mod hardware_tests {
    use super::{DeviceInfo, list_logitech_devices};

    #[test]
    fn detects_a_connected_g27() {
        let devices = list_logitech_devices().expect("énumération HID impossible");
        assert!(
            devices.iter().any(DeviceInfo::is_g27),
            "aucun G27 détecté — le volant est-il branché et accessible (HID : règle udev sous Linux) ?"
        );
    }
}
