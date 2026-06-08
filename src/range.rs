//! Réglage de l'angle de rotation du G27 (mode natif uniquement).
//!
//! La commande est un HID output report non numéroté de 7 octets, repris — à
//! titre de **référence documentaire**, aucune ligne de code n'est copiée — du
//! pilote Linux `drivers/hid/hid-lg4ff.c` (`lg4ff_set_range_g25`) :
//! `[0xF8, 0x81, range_lo, range_hi, 0x00, 0x00, 0x00]`, où `range_lo` et
//! `range_hi` encodent l'angle (en degrés) en little-endian.
//!
//! Le réglage n'a de sens qu'en **mode natif** (`0xC29B`) : en mode
//! compatibilité, le firmware ignore la commande. On exige donc un G27 natif et
//! on guide l'utilisateur vers `switch` le cas échéant.

use std::time::Duration;

use crate::hid;
use crate::report::{self, OutputReport};

/// En-tête de la commande de réglage d'angle (« set range »).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c` (`lg4ff_set_range_g25`).
const SET_RANGE_CMD: [u8; 2] = [0xF8, 0x81];

/// Angle de rotation minimal accepté par le G27 (degrés).
const MIN_RANGE_DEGREES: u16 = 40;

/// Angle de rotation maximal du G27 (degrés) — pleine échelle (2,5 tours).
const MAX_RANGE_DEGREES: u16 = 900;

/// Angle appliqué par défaut (pleine échelle), notamment automatiquement après
/// une bascule en mode natif.
pub const DEFAULT_RANGE_DEGREES: u16 = MAX_RANGE_DEGREES;

/// Résultat d'un réglage d'angle réussi.
#[derive(Debug, Clone)]
pub struct RangeOutcome {
    /// G27 natif ciblé.
    pub device: hid::DeviceInfo,
    /// Angle appliqué, en degrés.
    pub degrees: u16,
}

/// Erreurs du réglage d'angle.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// L'angle demandé est hors des bornes acceptées (40–900°).
    #[error("range must be between {MIN_RANGE_DEGREES} and {MAX_RANGE_DEGREES} degrees, got {0}")]
    OutOfRange(u16),
    /// Aucun G27 n'a été trouvé.
    #[error("no Logitech G27 was found")]
    NoG27Found,
    /// Un G27 est présent mais en mode compatibilité (réglage impossible).
    #[error("the G27 is in compatibility mode; switch to native mode first")]
    NotNative,
    /// Échec de construction ou d'envoi d'un report HID.
    #[error(transparent)]
    Report(#[from] report::Error),
}

/// Construit la commande HID réglant l'angle de rotation à `degrees`.
///
/// # Errors
///
/// Renvoie [`Error::OutOfRange`] si `degrees` n'est pas dans `[40, 900]`.
pub fn set_range_report(degrees: u16) -> Result<OutputReport, Error> {
    if !(MIN_RANGE_DEGREES..=MAX_RANGE_DEGREES).contains(&degrees) {
        return Err(Error::OutOfRange(degrees));
    }
    let range_lo = (degrees & 0x00FF) as u8;
    let range_hi = ((degrees >> 8) & 0x00FF) as u8;
    Ok(OutputReport::unnumbered(vec![
        SET_RANGE_CMD[0],
        SET_RANGE_CMD[1],
        range_lo,
        range_hi,
        0x00,
        0x00,
        0x00,
    ]))
}

/// Règle l'angle de rotation du G27 natif détecté.
///
/// # Errors
///
/// - [`Error::OutOfRange`] si l'angle est hors bornes ;
/// - [`Error::NoG27Found`] si aucun G27 n'est branché ;
/// - [`Error::NotNative`] si le G27 est en mode compatibilité ;
/// - [`Error::Report`] en cas d'échec d'initialisation hidapi ou d'envoi HID.
pub fn set_range(degrees: u16) -> Result<RangeOutcome, Error> {
    let api = hidapi::HidApi::new().map_err(report::Error::from)?;
    set_range_with_api(&api, degrees)
}

/// Règle l'angle de rotation en réutilisant une instance [`hidapi::HidApi`].
///
/// Variante de [`set_range`] pour les appelants possédant déjà un handle hidapi
/// persistant (session temps réel de la GUI).
///
/// # Errors
///
/// Identiques à [`set_range`] : [`Error::OutOfRange`], [`Error::NoG27Found`],
/// [`Error::NotNative`] ou [`Error::Report`].
pub fn set_range_with_api(api: &hidapi::HidApi, degrees: u16) -> Result<RangeOutcome, Error> {
    let report = set_range_report(degrees)?;
    let info = find_native_g27(api)?;
    report::send_reports(api, &info, std::slice::from_ref(&report), Duration::ZERO)?;
    tracing::info!("angle de rotation réglé à {degrees}°");
    Ok(RangeOutcome {
        device: info,
        degrees,
    })
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
    use super::{Error, set_range_report};

    #[test]
    fn builds_900_degree_packet() {
        // 900 = 0x0384 → lo=0x84, hi=0x03 ; préfixe 0x00 (pas de report ID).
        assert_eq!(
            set_range_report(900).expect("900° valide").to_buffer(),
            vec![0x00u8, 0xF8, 0x81, 0x84, 0x03, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn builds_270_degree_packet() {
        // 270 = 0x010E → lo=0x0E, hi=0x01.
        assert_eq!(
            set_range_report(270).expect("270° valide").to_buffer(),
            vec![0x00u8, 0xF8, 0x81, 0x0E, 0x01, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn accepts_lower_bound_40() {
        assert!(set_range_report(40).is_ok());
    }

    #[test]
    fn accepts_upper_bound_900() {
        assert!(set_range_report(900).is_ok());
    }

    #[test]
    fn rejects_below_lower_bound_39() {
        assert!(matches!(set_range_report(39), Err(Error::OutOfRange(39))));
    }

    #[test]
    fn rejects_above_upper_bound_901() {
        assert!(matches!(set_range_report(901), Err(Error::OutOfRange(901))));
    }

    #[test]
    fn rejects_zero() {
        assert!(matches!(set_range_report(0), Err(Error::OutOfRange(0))));
    }
}
