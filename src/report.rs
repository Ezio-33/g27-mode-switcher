//! Abstraction d'un HID output report et envoi vers un périphérique HID.
//!
//! Ce module factorise ce qui est commun à la bascule de mode (`switcher`) et au
//! réglage de l'angle de rotation (`range`) : la représentation d'une commande
//! Logitech (report ID + charge utile), sa validation, sa sérialisation en
//! buffer hidapi, et l'envoi d'une ou plusieurs commandes sur un périphérique
//! déjà identifié.
//!
//! Les commandes Logitech sont des reports non numérotés de 7 octets : le buffer
//! transmis à `HidDevice::write` est préfixé de `0x00` (« pas de report ID »,
//! retiré par hidapi et non transmis sur le fil).
//! Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`.

use std::time::Duration;

use crate::hid;

/// Octet de préfixe hidapi signifiant « pas de report ID numéroté » : hidapi le
/// retire et ne le transmet PAS sur le fil.
pub const HID_NO_REPORT_ID: u8 = 0x00;

/// Longueur d'une commande Logitech (corps du report, hors préfixe report ID).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c` (blocs de 7 octets).
pub const COMMAND_LEN: usize = 7;

/// Erreurs liées à la construction ou à l'envoi d'un report.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Échec d'une opération HID (ouverture du périphérique, écriture).
    #[error("HID operation failed: {0}")]
    Hid(#[from] hidapi::HidError),
    /// Le report construit ne respecte pas le format attendu par le firmware.
    #[error("invalid output report: {0}")]
    InvalidReport(&'static str),
}

/// Un HID output report : report ID suivi de sa charge utile.
///
/// La charge utile est possédée (`Vec<u8>`) afin de couvrir aussi bien les
/// commandes constantes (bascule de mode) que celles calculées au runtime
/// (réglage de l'angle). Sa longueur n'étant pas garantie par le type, elle est
/// contrôlée par [`OutputReport::validate`] avant tout envoi au matériel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputReport {
    /// Report ID HID (premier octet du buffer envoyé à `HidDevice::write`).
    pub report_id: u8,
    /// Corps du report.
    pub payload: Vec<u8>,
}

impl OutputReport {
    /// Construit un report non numéroté (préfixe `0x00`).
    #[must_use]
    pub fn unnumbered(payload: Vec<u8>) -> Self {
        Self {
            report_id: HID_NO_REPORT_ID,
            payload,
        }
    }

    /// Valide le report avant tout envoi au matériel.
    ///
    /// # Errors
    ///
    /// Renvoie [`Error::InvalidReport`] si la charge utile ne fait pas
    /// exactement [`COMMAND_LEN`] octets (format attendu par le firmware).
    pub fn validate(&self) -> Result<(), Error> {
        if self.payload.len() != COMMAND_LEN {
            return Err(Error::InvalidReport("report payload must be 7 bytes"));
        }
        Ok(())
    }

    /// Sérialise le report en buffer HID : `[report_id, payload…]`.
    #[must_use]
    pub fn to_buffer(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(1 + self.payload.len());
        buffer.push(self.report_id);
        buffer.extend_from_slice(&self.payload);
        buffer
    }
}

/// Ouvre le périphérique HID désigné et écrit la séquence de commandes.
///
/// Chaque commande est validée puis sérialisée ; les commandes successives sont
/// espacées de `inter_command_delay` (utile lorsqu'une commande déclenche une
/// reconnexion USB et que la suivante ne doit pas la précéder). Pour une
/// commande unique, le délai n'est jamais appliqué.
///
/// Aucune réclamation/libération d'interface n'est nécessaire : hidapi conserve
/// le pilote HID natif en place, l'envoi se résout à des `HidDevice::write`.
///
/// # Errors
///
/// - [`Error::InvalidReport`] si une commande est mal formée ;
/// - [`Error::Hid`] en cas d'échec d'ouverture du périphérique ou d'écriture.
pub fn send_reports(
    api: &hidapi::HidApi,
    info: &hid::DeviceInfo,
    reports: &[OutputReport],
    inter_command_delay: Duration,
) -> Result<(), Error> {
    for report in reports {
        report.validate()?;
    }

    // Sous Windows, un même périphérique peut exposer plusieurs collections HID :
    // on trace précisément celle ciblée afin de diagnostiquer un write qui
    // « réussit » sans que le firmware ne réagisse.
    tracing::debug!(
        "collection HID ciblée avant write : path={}, interface={}, usage_page={:#06x}, usage={:#06x}",
        info.path.to_string_lossy(),
        info.interface_number,
        info.usage_page,
        info.usage
    );

    let device = api.open_path(info.path.as_c_str())?;

    for (index, report) in reports.iter().enumerate() {
        if index > 0 {
            std::thread::sleep(inter_command_delay);
        }
        let buffer = report.to_buffer();
        let written = device.write(&buffer)?;
        tracing::debug!(
            "commande {}/{} écrite ({} octets) : {:02x?}",
            index + 1,
            reports.len(),
            written,
            buffer
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{HID_NO_REPORT_ID, OutputReport};

    #[test]
    fn unnumbered_uses_no_report_id() {
        let report = OutputReport::unnumbered(vec![0xF8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00]);
        assert_eq!(report.report_id, HID_NO_REPORT_ID);
    }

    #[test]
    fn to_buffer_prefixes_report_id() {
        let report = OutputReport::unnumbered(vec![0xF8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(
            report.to_buffer(),
            vec![0x00u8, 0xF8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn validate_accepts_seven_byte_payload() {
        let report = OutputReport::unnumbered(vec![0xF8, 0x81, 0x84, 0x03, 0x00, 0x00, 0x00]);
        assert!(report.validate().is_ok());
    }

    #[test]
    fn validate_rejects_wrong_length() {
        let report = OutputReport::unnumbered(vec![0xF8, 0x09]);
        assert!(report.validate().is_err());
    }
}
