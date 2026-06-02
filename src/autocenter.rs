//! Désactivation de l'autocentrage matériel du G27 (mode natif uniquement).
//!
//! En mode natif sans Logitech Gaming Software, le ressort de rappel au centre
//! géré par le firmware reste actif et **lutte contre le retour de force du
//! jeu**. LGS le désactivait au démarrage pour laisser le jeu gérer 100 % du
//! FFB ; on reproduit ce comportement.
//!
//! La commande est un HID output report non numéroté de 7 octets, repris — à
//! titre de **référence documentaire**, aucune ligne de code n'est copiée — du
//! pilote Linux `drivers/hid/hid-lg4ff.c` (`lg4ff_set_autocenter_default`, cas
//! `magnitude == 0`) : `[0xF5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]`.
//!
//! La **désactivation** (commande `0xF5`) et la **réactivation** à pleine force
//! (commandes `0xFE 0x0D` puis `0x14`) sont toutes deux exposées : le toggle de
//! l'interface est ainsi dynamique dans les deux sens, sans rebrancher le volant.

use std::time::Duration;

use crate::hid;
use crate::report::{self, OutputReport};

/// Commande de désactivation de l'autocentrage (« de-activate auto-center »).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c`
/// (`lg4ff_set_autocenter_default`, `magnitude == 0`).
const AUTOCENTER_DISABLE: [u8; report::COMMAND_LEN] = [0xF5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

/// 1ʳᵉ commande de réactivation : règle la force de l'autocentrage à pleine
/// amplitude. Réf. : `lg4ff_set_autocenter_default` pour `magnitude = 0xFFFF`,
/// avec `expand_a` divisé par deux pour les volants non-MOMO (cas du G27), d'où
/// les octets `0x07, 0x07, 0xFF`.
const AUTOCENTER_ENABLE_STRENGTH: [u8; report::COMMAND_LEN] =
    [0xFE, 0x0D, 0x07, 0x07, 0xFF, 0x00, 0x00];

/// 2ᵉ commande de réactivation : active l'autocentrage. Réf. : idem (`0x14`).
const AUTOCENTER_ENABLE_ACTIVATE: [u8; report::COMMAND_LEN] =
    [0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

/// Délai inséré entre les deux commandes de réactivation.
const ENABLE_COMMAND_DELAY: Duration = Duration::from_millis(10);

/// Résultat d'une désactivation réussie de l'autocentrage.
#[derive(Debug, Clone)]
pub struct AutocenterOutcome {
    /// G27 natif ciblé.
    pub device: hid::DeviceInfo,
}

/// Erreurs de la désactivation de l'autocentrage.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Aucun G27 n'a été trouvé.
    #[error("no Logitech G27 was found")]
    NoG27Found,
    /// Un G27 est présent mais en mode compatibilité (commande sans effet).
    #[error("the G27 is in compatibility mode; switch to native mode first")]
    NotNative,
    /// Échec de construction ou d'envoi d'un report HID.
    #[error(transparent)]
    Report(#[from] report::Error),
}

/// Construit la commande HID désactivant l'autocentrage matériel.
#[must_use]
pub fn disable_autocenter_report() -> OutputReport {
    OutputReport::unnumbered(AUTOCENTER_DISABLE.to_vec())
}

/// Désactive l'autocentrage matériel du G27 natif détecté.
///
/// # Errors
///
/// - [`Error::NoG27Found`] si aucun G27 n'est branché ;
/// - [`Error::NotNative`] si le G27 est en mode compatibilité ;
/// - [`Error::Report`] en cas d'échec d'initialisation hidapi ou d'envoi HID.
pub fn disable_autocenter() -> Result<AutocenterOutcome, Error> {
    let api = hidapi::HidApi::new().map_err(report::Error::from)?;
    disable_autocenter_with_api(&api)
}

/// Désactive l'autocentrage en réutilisant une instance [`hidapi::HidApi`].
///
/// Variante de [`disable_autocenter`] pour les appelants possédant déjà un
/// handle hidapi persistant (session temps réel de la GUI).
///
/// # Errors
///
/// Identiques à [`disable_autocenter`] : [`Error::NoG27Found`],
/// [`Error::NotNative`] ou [`Error::Report`].
pub fn disable_autocenter_with_api(api: &hidapi::HidApi) -> Result<AutocenterOutcome, Error> {
    let report = disable_autocenter_report();
    let info = find_native_g27(api)?;
    report::send_reports(api, &info, std::slice::from_ref(&report), Duration::ZERO)?;
    tracing::info!("autocentrage matériel désactivé");
    Ok(AutocenterOutcome { device: info })
}

/// Construit les deux commandes HID réactivant l'autocentrage (pleine force).
#[must_use]
pub fn enable_autocenter_reports() -> [OutputReport; 2] {
    [
        OutputReport::unnumbered(AUTOCENTER_ENABLE_STRENGTH.to_vec()),
        OutputReport::unnumbered(AUTOCENTER_ENABLE_ACTIVATE.to_vec()),
    ]
}

/// Réactive l'autocentrage matériel (pleine force) du G27 natif détecté.
///
/// # Errors
///
/// - [`Error::NoG27Found`] si aucun G27 n'est branché ;
/// - [`Error::NotNative`] si le G27 est en mode compatibilité ;
/// - [`Error::Report`] en cas d'échec d'initialisation hidapi ou d'envoi HID.
pub fn enable_autocenter() -> Result<AutocenterOutcome, Error> {
    let api = hidapi::HidApi::new().map_err(report::Error::from)?;
    enable_autocenter_with_api(&api)
}

/// Réactive l'autocentrage en réutilisant une instance [`hidapi::HidApi`].
///
/// Variante de [`enable_autocenter`] pour les appelants possédant déjà un handle
/// hidapi persistant (session temps réel de la GUI).
///
/// # Errors
///
/// Identiques à [`enable_autocenter`] : [`Error::NoG27Found`],
/// [`Error::NotNative`] ou [`Error::Report`].
pub fn enable_autocenter_with_api(api: &hidapi::HidApi) -> Result<AutocenterOutcome, Error> {
    let reports = enable_autocenter_reports();
    let info = find_native_g27(api)?;
    report::send_reports(api, &info, &reports, ENABLE_COMMAND_DELAY)?;
    tracing::info!("autocentrage matériel réactivé");
    Ok(AutocenterOutcome { device: info })
}

/// Recherche un G27 en mode natif dans l'énumération HID.
fn find_native_g27(api: &hidapi::HidApi) -> Result<hid::DeviceInfo, Error> {
    hid::find_native_g27(api).map_err(|reason| match reason {
        hid::NativeLookup::NotNative => Error::NotNative,
        hid::NativeLookup::NoG27 => Error::NoG27Found,
    })
}

#[cfg(test)]
mod tests {
    use super::disable_autocenter_report;

    #[test]
    fn builds_disable_packet() {
        // Préfixe 0x00 (« pas de report ID ») + commande 0xF5 de désactivation.
        assert_eq!(
            disable_autocenter_report().to_buffer(),
            vec![0x00u8, 0xF5, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn disable_packet_uses_no_report_id() {
        assert_eq!(disable_autocenter_report().report_id, 0x00);
    }

    #[test]
    fn builds_enable_packets() {
        let reports = super::enable_autocenter_reports();
        assert_eq!(
            reports[0].to_buffer(),
            vec![0x00u8, 0xFE, 0x0D, 0x07, 0x07, 0xFF, 0x00, 0x00]
        );
        assert_eq!(
            reports[1].to_buffer(),
            vec![0x00u8, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn enable_packets_use_no_report_id() {
        for report in super::enable_autocenter_reports() {
            assert_eq!(report.report_id, 0x00);
        }
    }
}
