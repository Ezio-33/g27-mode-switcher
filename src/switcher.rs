//! Construction et envoi de la séquence de bascule du G27 en mode natif.
//!
//! La bascule consiste en **deux HID output reports non numérotés** de 7 octets,
//! repris — à titre de **référence documentaire**, aucune ligne de code n'est
//! copiée — du pilote Linux `drivers/hid/hid-lg4ff.c`
//! (`lg4ff_mode_switch_ext09_g27`, `cmd_count = 2`) : « revert mode upon USB
//! reset » puis « switch to G27 with detach ». Attention à ne pas confondre avec
//! le G29 (`lg4ff_mode_switch_ext09_g29`), dont la 2ᵉ commande diffère
//! (`0x05, 0x01, 0x01` au lieu de `0x04, 0x01, 0x00`).
//!
//! Côté USB bas niveau, le noyau émet ces commandes comme des requêtes
//! `SET_REPORT` (classe HID). La représentation des reports et leur envoi sont
//! factorisés dans le module [`crate::report`] ; l'énumération et la recherche
//! du volant le sont dans [`crate::hid`].

use std::time::{Duration, Instant};

use crate::autocenter;
use crate::hid::{self, G27Mode};
use crate::range;
use crate::report::{self, OutputReport};

/// Charge utile « revert mode upon USB reset » (1ʳᵉ commande de la séquence).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c` (`lg4ff_mode_switch_ext09_g27`).
const MODE_SWITCH_REVERT_ON_RESET: [u8; report::COMMAND_LEN] =
    [0xF8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00];

/// Charge utile « switch to G27 with detach » (2ᵉ commande de la séquence).
/// Réf. : noyau Linux `drivers/hid/hid-lg4ff.c` (`lg4ff_mode_switch_ext09_g27`).
const MODE_SWITCH_TO_G27: [u8; report::COMMAND_LEN] = [0xF8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00];

/// Délai (en millisecondes) inséré entre les deux commandes de la séquence.
///
/// Le noyau Linux envoie les deux commandes d'affilée puis appelle
/// `hid_hw_wait`, qui draine la file des output reports avant de rendre la main.
/// En userspace via hidapi, `HidDevice::write` n'offre aucune garantie de
/// synchronisation équivalente : on insère un court délai pour laisser le
/// firmware traiter la 1ʳᵉ commande (revert-on-reset) avant d'émettre la 2ᵉ
/// (switch + detach), qui déclenche la déconnexion/reconnexion USB.
const INTER_COMMAND_DELAY_MS: u64 = 10;

/// Intervalle entre deux scrutations de l'énumération HID pendant l'attente de
/// la reconnexion du volant en mode natif (millisecondes).
const RECONNECT_POLL_INTERVAL_MS: u64 = 200;

/// Durée maximale d'attente de la réapparition du G27 en mode natif après la
/// bascule, avant d'abandonner le réglage automatique de l'angle (millisecondes).
const RECONNECT_TIMEOUT_MS: u64 = 6000;

/// Construit la séquence de bascule du G27 en mode natif.
///
/// Deux commandes successives, à l'image du noyau Linux
/// (`lg4ff_mode_switch_ext09_g27`, `cmd_count = 2`) :
/// 1. « revert mode upon USB reset » ;
/// 2. « switch to G27 with detach » (déclenche la reconnexion).
#[must_use]
pub fn mode_switch_sequence() -> [OutputReport; 2] {
    [
        OutputReport::unnumbered(MODE_SWITCH_REVERT_ON_RESET.to_vec()),
        OutputReport::unnumbered(MODE_SWITCH_TO_G27.to_vec()),
    ]
}

/// Issue du réglage automatique de l'angle de rotation après la bascule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeStep {
    /// Réglage non tenté : simulation (`--dry-run`) ou option `--no-range`.
    Skipped,
    /// Angle réglé automatiquement à la valeur indiquée (degrés).
    Applied(u16),
    /// Bascule réussie, mais l'angle n'a pas pu être réglé automatiquement
    /// (volant non réapparu à temps ou échec d'envoi) : à faire manuellement.
    Deferred(u16),
}

/// Issue de la désactivation automatique de l'autocentrage après la bascule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutocenterStep {
    /// Désactivation non tentée : simulation (`--dry-run`) ou `--no-autocenter`.
    Skipped,
    /// Autocentrage matériel désactivé automatiquement.
    Disabled,
    /// Bascule réussie, mais l'autocentrage n'a pas pu être désactivé
    /// automatiquement (volant non réapparu à temps ou échec d'envoi).
    Deferred,
}

/// Résultat d'une bascule réussie (ou simulée).
#[derive(Debug, Clone)]
pub struct SwitchOutcome {
    /// G27 ciblé, dans son état détecté avant la bascule (mode compatibilité).
    pub device: hid::DeviceInfo,
    /// Vrai si l'envoi a été simulé (aucun octet réellement transmis).
    pub dry_run: bool,
    /// Issue du réglage automatique de l'angle de rotation.
    pub range: RangeStep,
    /// Issue de la désactivation automatique de l'autocentrage.
    pub autocenter: AutocenterStep,
}

/// Erreurs de la bascule de mode.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Échec de construction ou d'envoi d'un report HID.
    #[error(transparent)]
    Report(#[from] report::Error),
    /// Aucun G27 n'a été trouvé.
    #[error("no Logitech G27 was found")]
    NoG27Found,
    /// Un G27 est présent mais déjà en mode natif.
    #[error("a G27 was found but is already in native mode")]
    AlreadyNative,
}

/// Bascule le premier G27 détecté en mode compatibilité vers le mode natif.
///
/// En `dry_run`, la séquence est construite et validée, mais aucun octet n'est
/// envoyé au matériel. Hors `dry_run`, une fois le volant réapparu en mode
/// natif, l'outil applique les réglages demandés : l'angle de rotation à
/// [`range::DEFAULT_RANGE_DEGREES`] (900°) si `apply_range`, puis — uniquement
/// si `disable_autocenter` — la désactivation de l'autocentrage matériel.
///
/// Par défaut, l'autocentrage matériel est **laissé actif** : sans FFB dynamique
/// (indisponible en HID natif sans pilote), il constitue la seule force de
/// centrage du volant. Le désactiver ne se justifie que si une couche FFB prend
/// le relais. Si le volant ne réapparaît pas à temps, la bascule reste un succès
/// et chaque réglage concerné est reporté comme « différé »
/// ([`RangeStep::Deferred`], [`AutocenterStep::Deferred`]).
///
/// # Errors
///
/// - [`Error::NoG27Found`] ou [`Error::AlreadyNative`] selon l'état détecté ;
/// - [`Error::Report`] si une commande est invalide ou si l'envoi HID échoue
///   (initialisation hidapi, ouverture du périphérique, écriture).
pub fn switch_to_native_mode(
    dry_run: bool,
    apply_range: bool,
    disable_autocenter: bool,
) -> Result<SwitchOutcome, Error> {
    let mut api = hidapi::HidApi::new().map_err(report::Error::from)?;
    switch_with_api(&mut api, dry_run, apply_range, disable_autocenter)
}

/// Bascule en mode natif en réutilisant une instance [`hidapi::HidApi`] fournie.
///
/// Variante de [`switch_to_native_mode`] destinée aux appelants qui possèdent
/// déjà un handle hidapi persistant (p. ex. la session temps réel de la GUI),
/// afin de ne pas réinitialiser le sous-système HID à chaque opération.
///
/// # Errors
///
/// Identiques à [`switch_to_native_mode`] : [`Error::NoG27Found`],
/// [`Error::AlreadyNative`] ou [`Error::Report`].
pub fn switch_with_api(
    api: &mut hidapi::HidApi,
    dry_run: bool,
    apply_range: bool,
    disable_autocenter: bool,
) -> Result<SwitchOutcome, Error> {
    let sequence = mode_switch_sequence();
    for command in &sequence {
        command.validate()?;
    }

    let info = find_compat_g27(api)?;

    if dry_run {
        tracing::info!(
            "simulation : séquence de {} commandes construite et validée, aucun octet envoyé",
            sequence.len()
        );
        return Ok(SwitchOutcome {
            device: info,
            dry_run: true,
            range: RangeStep::Skipped,
            autocenter: AutocenterStep::Skipped,
        });
    }

    report::send_reports(
        api,
        &info,
        &sequence,
        Duration::from_millis(INTER_COMMAND_DELAY_MS),
    )?;
    tracing::info!(
        "séquence de bascule envoyée ({} commandes) ; le G27 va se reconnecter",
        sequence.len()
    );

    let (range, autocenter) = apply_post_switch_settings(api, apply_range, disable_autocenter);

    Ok(SwitchOutcome {
        device: info,
        dry_run: false,
        range,
        autocenter,
    })
}

/// Recherche un G27 en mode compatibilité dans l'énumération HID.
fn find_compat_g27(api: &hidapi::HidApi) -> Result<hid::DeviceInfo, Error> {
    hid::find_g27(api, G27Mode::Compatibility).map_err(|other| match other {
        Some(G27Mode::Native) => Error::AlreadyNative,
        _ => Error::NoG27Found,
    })
}

/// Attend la réapparition du G27 en mode natif (après le detach/reconnect) puis
/// applique les réglages demandés, dans l'ordre de LGS : angle de rotation par
/// défaut, puis désactivation de l'autocentrage matériel.
///
/// Ne renvoie jamais d'erreur : la bascule ayant déjà eu lieu, tout échec (ou un
/// volant qui tarde à réapparaître) est tracé et reporté comme « différé », afin
/// que l'utilisateur rejoue le réglage manquant via `set-range` / `set-autocenter`.
fn apply_post_switch_settings(
    api: &mut hidapi::HidApi,
    apply_range: bool,
    disable_autocenter: bool,
) -> (RangeStep, AutocenterStep) {
    if !apply_range && !disable_autocenter {
        return (RangeStep::Skipped, AutocenterStep::Skipped);
    }

    let degrees = range::DEFAULT_RANGE_DEGREES;
    let Some(native) = wait_for_native_g27(api) else {
        tracing::warn!("le G27 n'est pas réapparu en mode natif à temps ; réglages différés");
        return (
            if apply_range {
                RangeStep::Deferred(degrees)
            } else {
                RangeStep::Skipped
            },
            if disable_autocenter {
                AutocenterStep::Deferred
            } else {
                AutocenterStep::Skipped
            },
        );
    };

    let range_step = if apply_range {
        match send_default_range(api, &native, degrees) {
            Ok(()) => RangeStep::Applied(degrees),
            Err(error) => {
                tracing::warn!("réglage automatique de l'angle échoué : {error} ; différé");
                RangeStep::Deferred(degrees)
            }
        }
    } else {
        RangeStep::Skipped
    };

    let autocenter_step = if disable_autocenter {
        let report = autocenter::disable_autocenter_report();
        match report::send_reports(api, &native, std::slice::from_ref(&report), Duration::ZERO) {
            Ok(()) => AutocenterStep::Disabled,
            Err(error) => {
                tracing::warn!(
                    "désactivation automatique de l'autocentrage échouée : {error} ; différée"
                );
                AutocenterStep::Deferred
            }
        }
    } else {
        AutocenterStep::Skipped
    };

    (range_step, autocenter_step)
}

/// Scrute l'énumération HID jusqu'à voir le G27 en mode natif, ou jusqu'au
/// délai [`RECONNECT_TIMEOUT_MS`].
fn wait_for_native_g27(api: &mut hidapi::HidApi) -> Option<hid::DeviceInfo> {
    let deadline = Instant::now() + Duration::from_millis(RECONNECT_TIMEOUT_MS);
    loop {
        if let Err(error) = api.refresh_devices() {
            tracing::debug!("re-énumération HID impossible : {error}");
        } else if let Ok(native) = hid::find_g27(api, G27Mode::Native) {
            return Some(native);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(RECONNECT_POLL_INTERVAL_MS));
    }
}

/// Construit et envoie la commande de réglage d'angle par défaut sur le volant
/// natif fraîchement reconnecté.
fn send_default_range(
    api: &hidapi::HidApi,
    native: &hid::DeviceInfo,
    degrees: u16,
) -> Result<(), report::Error> {
    let report = range::set_range_report(degrees)
        .map_err(|_| report::Error::InvalidReport("default range out of bounds"))?;
    report::send_reports(api, native, std::slice::from_ref(&report), Duration::ZERO)
}

#[cfg(test)]
mod tests {
    use super::mode_switch_sequence;

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
}
