//! Construction et envoi du « magic packet » qui bascule le G27 en mode natif.
//!
//! Le packet est un **HID output report** (report ID 3). Côté USB bas niveau,
//! il correspond à la requête `SET_REPORT` documentée dans le pilote Linux
//! `drivers/hid/hid-lg4ff.c` (`bmRequestType = 0x21`, `bRequest = 0x09`,
//! `wValue = 0x0203`) — repris à titre de **référence documentaire**, aucune
//! ligne de code n'est copiée. En passant par l'API HID native (`hidapi`),
//! l'envoi se fait via `HidDevice::write`, sans déposséder le pilote HID du
//! système (donc sans Zadig/WinUSB).

use crate::hid::{self, LOGITECH_VENDOR_ID};

/// Report ID de l'output report de bascule (octet de poids faible de
/// `wValue = 0x0203` dans le transfert `SET_REPORT` du noyau Linux).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`.
const MODE_SWITCH_REPORT_ID: u8 = 0x03;

/// Charge utile du magic packet de bascule de mode (corps du report).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`.
const MODE_SWITCH_PAYLOAD: [u8; 7] = [0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00];

/// Un HID output report : report ID suivi de sa charge utile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputReport {
    /// Report ID HID (premier octet du buffer envoyé à `HidDevice::write`).
    pub report_id: u8,
    /// Corps du report.
    pub payload: &'static [u8],
}

impl OutputReport {
    /// Valide le report avant tout envoi au matériel.
    ///
    /// # Errors
    ///
    /// Renvoie [`Error::InvalidReport`] si le report ID est nul (un report ID
    /// de 0 signifie « pas de report numéroté » et ne doit pas être préfixé)
    /// ou si la charge utile est vide.
    pub fn validate(&self) -> Result<(), Error> {
        if self.report_id == 0 {
            return Err(Error::InvalidReport("report id must not be zero"));
        }
        if self.payload.is_empty() {
            return Err(Error::InvalidReport("report payload must not be empty"));
        }
        Ok(())
    }

    /// Sérialise le report en buffer HID : `[report_id, payload…]`.
    #[must_use]
    pub fn to_buffer(self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(1 + self.payload.len());
        buffer.push(self.report_id);
        buffer.extend_from_slice(self.payload);
        buffer
    }
}

/// Construit l'output report qui bascule le G27 en mode natif.
#[must_use]
pub fn mode_switch_report() -> OutputReport {
    OutputReport {
        report_id: MODE_SWITCH_REPORT_ID,
        payload: &MODE_SWITCH_PAYLOAD,
    }
}

/// Résultat d'une bascule réussie (ou simulée).
#[derive(Debug, Clone)]
pub struct SwitchOutcome {
    /// G27 ciblé, dans son état détecté avant la bascule (mode compatibilité).
    pub device: hid::DeviceInfo,
    /// Vrai si l'envoi a été simulé (aucun octet réellement transmis).
    pub dry_run: bool,
}

/// Erreurs de la bascule de mode.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Échec d'une opération HID (initialisation, ouverture, écriture).
    #[error("HID operation failed: {0}")]
    Hid(#[from] hidapi::HidError),
    /// Aucun G27 n'a été trouvé.
    #[error("no Logitech G27 was found")]
    NoG27Found,
    /// Un G27 est présent mais déjà en mode natif.
    #[error("a G27 was found but is already in native mode")]
    AlreadyNative,
    /// Le report construit est invalide.
    #[error("invalid output report: {0}")]
    InvalidReport(&'static str),
}

/// Bascule le premier G27 détecté en mode compatibilité vers le mode natif.
///
/// En `dry_run`, le report est construit et validé, mais aucun octet n'est
/// envoyé au matériel.
///
/// # Errors
///
/// - [`Error::NoG27Found`] ou [`Error::AlreadyNative`] selon l'état détecté ;
/// - [`Error::InvalidReport`] si le report construit est invalide ;
/// - [`Error::Hid`] en cas d'échec d'initialisation hidapi, d'ouverture du
///   périphérique ou d'écriture du report.
pub fn switch_to_native_mode(dry_run: bool) -> Result<SwitchOutcome, Error> {
    let report = mode_switch_report();
    report.validate()?;

    let api = hidapi::HidApi::new()?;
    let info = find_compat_g27(&api)?;

    if dry_run {
        tracing::info!("simulation : output report construit et validé, aucun octet envoyé");
        return Ok(SwitchOutcome {
            device: info,
            dry_run: true,
        });
    }

    send_report(&api, &info, &report)?;
    Ok(SwitchOutcome {
        device: info,
        dry_run: false,
    })
}

/// Recherche un G27 en mode compatibilité dans l'énumération HID.
fn find_compat_g27(api: &hidapi::HidApi) -> Result<hid::DeviceInfo, Error> {
    let mut native_seen = false;

    for entry in api.device_list() {
        if entry.vendor_id() != LOGITECH_VENDOR_ID {
            continue;
        }
        match hid::classify_product_id(entry.product_id()) {
            hid::G27Mode::Compatibility => return Ok(hid::device_info_from(entry)),
            hid::G27Mode::Native => native_seen = true,
            hid::G27Mode::Other => {}
        }
    }

    if native_seen {
        Err(Error::AlreadyNative)
    } else {
        Err(Error::NoG27Found)
    }
}

/// Ouvre le périphérique HID et écrit l'output report de bascule.
///
/// Aucune réclamation/libération d'interface n'est nécessaire : hidapi conserve
/// le pilote HID natif en place et l'envoi se résout à un `HidDevice::write`.
fn send_report(
    api: &hidapi::HidApi,
    info: &hid::DeviceInfo,
    report: &OutputReport,
) -> Result<(), Error> {
    // Sous Windows, un même G27 peut exposer plusieurs collections HID : on
    // trace précisément celle que l'on cible afin de diagnostiquer une bascule
    // qui « réussit » (write OK) sans que le firmware ne réagisse.
    tracing::debug!(
        "collection HID ciblée avant write : path={}, interface={}, usage_page={:#06x}, usage={:#06x}",
        info.path.to_string_lossy(),
        info.interface_number,
        info.usage_page,
        info.usage
    );

    let device = api.open_path(info.path.as_c_str())?;
    let buffer = report.to_buffer();
    let written = device.write(&buffer)?;
    tracing::info!("output report envoyé ({written} octets) ; le G27 va se reconnecter");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Error, OutputReport, mode_switch_report};

    #[test]
    fn builds_expected_magic_packet() {
        let report = mode_switch_report();
        assert_eq!(report.report_id, 0x03);
        assert_eq!(
            report.payload,
            [0xF8u8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00].as_slice()
        );
    }

    #[test]
    fn buffer_prefixes_report_id() {
        assert_eq!(
            mode_switch_report().to_buffer(),
            vec![0x03u8, 0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00]
        );
    }

    #[test]
    fn default_report_is_valid() {
        assert!(mode_switch_report().validate().is_ok());
    }

    #[test]
    fn rejects_zero_report_id() {
        let report = OutputReport {
            report_id: 0x00,
            ..mode_switch_report()
        };
        assert!(matches!(report.validate(), Err(Error::InvalidReport(_))));
    }

    #[test]
    fn rejects_empty_payload() {
        let report = OutputReport {
            payload: &[],
            ..mode_switch_report()
        };
        assert!(matches!(report.validate(), Err(Error::InvalidReport(_))));
    }
}
