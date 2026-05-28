//! Détection des périphériques Logitech et identification du mode du G27.
//!
//! Ce module se limite à l'énumération et à l'analyse des descripteurs USB
//! (lecture seule, sans ouverture du périphérique, donc sans privilèges
//! élevés). La bascule de mode proprement dite relève du module `switcher`.

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

/// Informations sur un périphérique Logitech détecté.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Vendor ID (toujours [`LOGITECH_VENDOR_ID`] ici).
    pub vendor_id: u16,
    /// Product ID brut tel qu'annoncé par le périphérique.
    pub product_id: u16,
    /// Numéro de bus USB.
    pub bus_number: u8,
    /// Adresse du périphérique sur le bus.
    pub address: u8,
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
            "{label} — VID {:#06x} PID {:#06x} (bus {}, adresse {})",
            self.vendor_id, self.product_id, self.bus_number, self.address
        )
    }
}

/// Erreurs du module USB.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Échec d'accès au sous-système USB (initialisation libusb, énumération…).
    #[error("failed to access the USB subsystem: {0}")]
    Access(#[from] rusb::Error),
}

/// Énumère les périphériques Logitech actuellement connectés.
///
/// L'opération lit uniquement les descripteurs ; elle n'ouvre aucun
/// périphérique et ne requiert donc pas de privilèges élevés.
///
/// # Errors
///
/// Renvoie [`Error::Access`] si le sous-système USB ne peut pas être
/// interrogé (libusb indisponible, énumération impossible, lecture d'un
/// descripteur en échec).
pub fn list_logitech_devices() -> Result<Vec<DeviceInfo>, Error> {
    let mut devices = Vec::new();

    for device in rusb::devices()?.iter() {
        let descriptor = device.device_descriptor()?;
        if descriptor.vendor_id() != LOGITECH_VENDOR_ID {
            continue;
        }

        let product_id = descriptor.product_id();
        let info = DeviceInfo {
            vendor_id: descriptor.vendor_id(),
            product_id,
            bus_number: device.bus_number(),
            address: device.address(),
            mode: classify_product_id(product_id),
        };

        tracing::debug!(
            mode = ?info.mode,
            "périphérique Logitech détecté : VID {:#06x}, PID {:#06x}",
            info.vendor_id,
            info.product_id
        );
        devices.push(info);
    }

    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceInfo, G27_COMPAT_PRODUCT_ID, G27_NATIVE_PRODUCT_ID, G27Mode, LOGITECH_VENDOR_ID,
        classify_product_id,
    };

    fn device(product_id: u16) -> DeviceInfo {
        DeviceInfo {
            vendor_id: LOGITECH_VENDOR_ID,
            product_id,
            bus_number: 1,
            address: 2,
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
}
