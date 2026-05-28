//! Construction et envoi du « magic packet » qui bascule le G27 en mode natif.
//!
//! Le format du transfert de contrôle est repris, à titre de référence
//! documentaire uniquement, du pilote Linux `drivers/hid/hid-lg4ff.c`
//! (aucune ligne de code n'est copiée ; le comportement est réimplémenté).

use std::time::Duration;

use crate::usb::{self, LOGITECH_VENDOR_ID};

/// `bmRequestType` : Host-to-Device | Class | Interface.
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`.
const REQUEST_TYPE: u8 = 0x21;

/// `bRequest` : `SET_REPORT` (classe HID).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`.
const REQUEST_SET_REPORT: u8 = 0x09;

/// `wValue` : report de type Output (`0x02`) combiné au report ID 3 (`0x03`).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`.
const VALUE_OUTPUT_REPORT_3: u16 = 0x0203;

/// `wIndex` : interface 0.
const INTERFACE_INDEX: u16 = 0x0000;

/// Numéro de l'interface USB à réclamer pour émettre le transfert.
const INTERFACE_NUMBER: u8 = 0x00;

/// Octets du magic packet de bascule de mode.
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`.
const MODE_SWITCH_PAYLOAD: [u8; 7] = [0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00];

/// Délai maximal accordé au transfert de contrôle.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(1);

/// Bit de direction de `bmRequestType` (1 = Device-to-Host / IN).
const DIRECTION_IN_MASK: u8 = 0x80;

/// Paramètres d'un transfert de contrôle USB (requête `SET_REPORT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlRequest {
    /// `bmRequestType`.
    pub request_type: u8,
    /// `bRequest`.
    pub request: u8,
    /// `wValue`.
    pub value: u16,
    /// `wIndex`.
    pub index: u16,
    /// Charge utile (corps du report).
    pub data: &'static [u8],
}

impl ControlRequest {
    /// Valide le transfert avant tout envoi au matériel.
    ///
    /// # Errors
    ///
    /// Renvoie [`Error::InvalidRequest`] si la direction n'est pas
    /// Host-to-Device (OUT) ou si la charge utile est vide.
    pub fn validate(&self) -> Result<(), Error> {
        if self.request_type & DIRECTION_IN_MASK != 0 {
            return Err(Error::InvalidRequest(
                "transfer direction must be host-to-device (out)",
            ));
        }
        if self.data.is_empty() {
            return Err(Error::InvalidRequest("report payload must not be empty"));
        }
        Ok(())
    }
}

/// Construit le transfert de contrôle qui bascule le G27 en mode natif.
#[must_use]
pub fn mode_switch_request() -> ControlRequest {
    ControlRequest {
        request_type: REQUEST_TYPE,
        request: REQUEST_SET_REPORT,
        value: VALUE_OUTPUT_REPORT_3,
        index: INTERFACE_INDEX,
        data: &MODE_SWITCH_PAYLOAD,
    }
}

/// Résultat d'une bascule réussie (ou simulée).
#[derive(Debug, Clone)]
pub struct SwitchOutcome {
    /// G27 ciblé, dans son état détecté avant la bascule (mode compatibilité).
    pub device: usb::DeviceInfo,
    /// Vrai si l'envoi a été simulé (aucun octet réellement transmis).
    pub dry_run: bool,
}

/// Erreurs de la bascule de mode.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Échec d'une opération USB (ouverture, interface, transfert).
    #[error("USB operation failed: {0}")]
    Usb(#[from] rusb::Error),
    /// Aucun G27 n'a été trouvé.
    #[error("no Logitech G27 was found")]
    NoG27Found,
    /// Un G27 est présent mais déjà en mode natif.
    #[error("a G27 was found but is already in native mode")]
    AlreadyNative,
    /// Le transfert construit est invalide.
    #[error("invalid control request: {0}")]
    InvalidRequest(&'static str),
}

/// Bascule le premier G27 détecté en mode compatibilité vers le mode natif.
///
/// En `dry_run`, le transfert est construit et validé, mais aucun octet n'est
/// envoyé au matériel.
///
/// # Errors
///
/// - [`Error::NoG27Found`] ou [`Error::AlreadyNative`] selon l'état détecté ;
/// - [`Error::InvalidRequest`] si le transfert construit est invalide ;
/// - [`Error::Usb`] en cas d'échec d'ouverture, de réclamation d'interface ou
///   d'envoi du transfert de contrôle.
pub fn switch_to_native_mode(dry_run: bool) -> Result<SwitchOutcome, Error> {
    let request = mode_switch_request();
    request.validate()?;

    let (device, info) = find_compat_g27()?;

    if dry_run {
        tracing::info!("simulation : magic packet construit et validé, aucun octet envoyé");
        return Ok(SwitchOutcome {
            device: info,
            dry_run: true,
        });
    }

    send_control(&device, &request)?;
    Ok(SwitchOutcome {
        device: info,
        dry_run: false,
    })
}

/// Recherche un G27 en mode compatibilité et renvoie son périphérique ouvrable.
fn find_compat_g27() -> Result<(rusb::Device<rusb::GlobalContext>, usb::DeviceInfo), Error> {
    let mut native_seen = false;

    for device in rusb::devices()?.iter() {
        let descriptor = device.device_descriptor()?;
        if descriptor.vendor_id() != LOGITECH_VENDOR_ID {
            continue;
        }

        let product_id = descriptor.product_id();
        match usb::classify_product_id(product_id) {
            usb::G27Mode::Compatibility => {
                let info = usb::DeviceInfo {
                    vendor_id: descriptor.vendor_id(),
                    product_id,
                    bus_number: device.bus_number(),
                    address: device.address(),
                    mode: usb::G27Mode::Compatibility,
                };
                return Ok((device, info));
            }
            usb::G27Mode::Native => native_seen = true,
            usb::G27Mode::Other => {}
        }
    }

    if native_seen {
        Err(Error::AlreadyNative)
    } else {
        Err(Error::NoG27Found)
    }
}

/// Ouvre le périphérique et émet le transfert de contrôle de bascule.
fn send_control(
    device: &rusb::Device<rusb::GlobalContext>,
    request: &ControlRequest,
) -> Result<(), Error> {
    let handle = device.open()?;

    // Sous Linux, le pilote noyau usbhid peut être attaché à l'interface : on
    // tente de le détacher automatiquement. Non supporté sous Windows (où
    // WinUSB est déjà en place) ; l'échec est ignoré volontairement.
    if let Err(error) = handle.set_auto_detach_kernel_driver(true) {
        tracing::debug!(%error, "détachement auto du pilote noyau indisponible");
    }

    handle.claim_interface(INTERFACE_NUMBER)?;

    let written = handle.write_control(
        request.request_type,
        request.request,
        request.value,
        request.index,
        request.data,
        TRANSFER_TIMEOUT,
    )?;

    tracing::info!("magic packet envoyé ({written} octets) ; le G27 va se reconnecter");

    // Le périphérique se réinitialise : la libération peut échouer (device déjà
    // parti), ce qui n'est pas une erreur.
    let _ = handle.release_interface(INTERFACE_NUMBER);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ControlRequest, Error, mode_switch_request};

    #[test]
    fn builds_expected_magic_packet() {
        let request = mode_switch_request();
        assert_eq!(request.request_type, 0x21);
        assert_eq!(request.request, 0x09);
        assert_eq!(request.value, 0x0203);
        assert_eq!(request.index, 0x0000);
        assert_eq!(
            request.data,
            [0xF8u8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00].as_slice()
        );
    }

    #[test]
    fn default_request_is_valid() {
        assert!(mode_switch_request().validate().is_ok());
    }

    #[test]
    fn rejects_in_direction() {
        let request = ControlRequest {
            request_type: 0xA1,
            ..mode_switch_request()
        };
        assert!(matches!(request.validate(), Err(Error::InvalidRequest(_))));
    }

    #[test]
    fn rejects_empty_payload() {
        let request = ControlRequest {
            data: &[],
            ..mode_switch_request()
        };
        assert!(matches!(request.validate(), Err(Error::InvalidRequest(_))));
    }
}
