//! Construction et envoi du « magic packet » qui bascule le G27 en mode natif.
//!
//! Le packet est un **HID output report** (report ID 3). Côté USB bas niveau,
//! il correspond à la requête `SET_REPORT` documentée dans le pilote Linux
//! `drivers/hid/hid-lg4ff.c` (`bmRequestType = 0x21`, `bRequest = 0x09`,
//! `wValue = 0x0203`) — repris à titre de **référence documentaire**, aucune
//! ligne de code n'est copiée. En passant par l'API HID native (`hidapi`),
//! l'envoi se fait via `HidDevice::write`, sans déposséder le pilote HID du
//! système (donc sans Zadig/WinUSB).

use std::time::Duration;

use crate::hid::{self, LOGITECH_VENDOR_ID};

/// Octet de préfixe hidapi signifiant « pas de report ID numéroté » : hidapi le
/// retire et ne le transmet PAS sur le fil. Les commandes Logitech de bascule
/// sont des reports non numérotés de 7 octets.
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c` (les commandes y sont des
/// blocs de 7 octets, sans report ID préfixé).
const HID_NO_REPORT_ID: u8 = 0x00;

/// Longueur d'une commande de bascule (corps du report, hors préfixe).
const COMMAND_LEN: usize = 7;

/// Charge utile « revert mode upon USB reset » (1ʳᵉ commande de la séquence).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c` (`lg4ff_mode_switch_ext09_g27`).
const MODE_SWITCH_REVERT_ON_RESET: [u8; COMMAND_LEN] = [0xF8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00];

/// Charge utile « switch to G27 with detach » (2ᵉ commande de la séquence).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c` (`lg4ff_mode_switch_ext09_g27`).
const MODE_SWITCH_TO_G27: [u8; COMMAND_LEN] = [0xF8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00];

/// Délai (en millisecondes) inséré entre les deux commandes de la séquence.
///
/// Le noyau Linux envoie les deux commandes d'affilée puis appelle
/// `hid_hw_wait`, qui draine la file des output reports avant de rendre la main.
/// En userspace via hidapi, `HidDevice::write` n'offre aucune garantie de
/// synchronisation équivalente : on insère un court délai pour laisser le
/// firmware traiter la 1ʳᵉ commande (revert-on-reset) avant d'émettre la 2ᵉ
/// (switch + detach), qui déclenche la déconnexion/reconnexion USB.
const INTER_COMMAND_DELAY_MS: u64 = 10;

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
    pub fn to_buffer(self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(1 + self.payload.len());
        buffer.push(self.report_id);
        buffer.extend_from_slice(self.payload);
        buffer
    }
}

/// Construit la séquence de bascule du G27 en mode natif.
///
/// Deux commandes successives, à l'image du noyau Linux
/// (`lg4ff_mode_switch_ext09_g27`, `cmd_count = 2`) :
/// 1. « revert mode upon USB reset » ;
/// 2. « switch to G27 with detach » (déclenche la reconnexion).
#[must_use]
pub fn mode_switch_sequence() -> [OutputReport; 2] {
    [
        OutputReport {
            report_id: HID_NO_REPORT_ID,
            payload: &MODE_SWITCH_REVERT_ON_RESET,
        },
        OutputReport {
            report_id: HID_NO_REPORT_ID,
            payload: &MODE_SWITCH_TO_G27,
        },
    ]
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
/// - [`Error::InvalidReport`] si une commande construite est invalide ;
/// - [`Error::Hid`] en cas d'échec d'initialisation hidapi, d'ouverture du
///   périphérique ou d'écriture d'une commande.
pub fn switch_to_native_mode(dry_run: bool) -> Result<SwitchOutcome, Error> {
    let sequence = mode_switch_sequence();
    for command in &sequence {
        command.validate()?;
    }

    let api = hidapi::HidApi::new()?;
    let info = find_compat_g27(&api)?;

    if dry_run {
        tracing::info!(
            "simulation : séquence de {} commandes construite et validée, aucun octet envoyé",
            sequence.len()
        );
        return Ok(SwitchOutcome {
            device: info,
            dry_run: true,
        });
    }

    send_sequence(&api, &info, &sequence)?;
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

/// Ouvre le périphérique HID et écrit la séquence de commandes de bascule.
///
/// Aucune réclamation/libération d'interface n'est nécessaire : hidapi conserve
/// le pilote HID natif en place et l'envoi se résout à des `HidDevice::write`
/// successifs, espacés de [`INTER_COMMAND_DELAY_MS`].
fn send_sequence(
    api: &hidapi::HidApi,
    info: &hid::DeviceInfo,
    sequence: &[OutputReport],
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
    let delay = Duration::from_millis(INTER_COMMAND_DELAY_MS);

    for (index, command) in sequence.iter().enumerate() {
        if index > 0 {
            std::thread::sleep(delay);
        }
        let buffer = command.to_buffer();
        let written = device.write(&buffer)?;
        tracing::debug!(
            "commande {}/{} écrite ({} octets) : {:02x?}",
            index + 1,
            sequence.len(),
            written,
            buffer
        );
    }

    tracing::info!(
        "séquence de bascule envoyée ({} commandes) ; le G27 va se reconnecter",
        sequence.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Error, OutputReport, mode_switch_sequence};

    #[test]
    fn sequence_has_two_commands() {
        assert_eq!(mode_switch_sequence().len(), 2);
    }

    #[test]
    fn first_command_reverts_on_reset() {
        // Préfixe 0x00 (« pas de report ID ») + revert-on-reset.
        assert_eq!(
            mode_switch_sequence()[0].to_buffer(),
            vec![0x00u8, 0xF8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn second_command_switches_to_g27() {
        // Payload G27 (et non G29 : 0x04, pas 0x05/0x01/0x01), sans report ID.
        assert_eq!(
            mode_switch_sequence()[1].to_buffer(),
            vec![0x00u8, 0xF8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn all_commands_use_no_report_id_and_are_valid() {
        for command in mode_switch_sequence() {
            assert_eq!(command.report_id, 0x00);
            assert!(command.validate().is_ok());
        }
    }

    #[test]
    fn rejects_wrong_length() {
        let command = OutputReport {
            payload: &[],
            ..mode_switch_sequence()[1]
        };
        assert!(matches!(command.validate(), Err(Error::InvalidReport(_))));
    }
}
