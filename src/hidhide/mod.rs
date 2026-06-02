//! Masquage du G27 réel au jeu via HidHide (IOCTL direct sur `\\.\HidHide`).
//!
//! Quand le feeder recopie le G27 vers vJoy, le jeu pourrait voir **à la fois**
//! le volant réel et le device vJoy (doubles entrées). HidHide cache le volant
//! réel à toutes les applications **sauf** celles de sa liste blanche : on y
//! inscrit notre propre exécutable (pour continuer à lire le G27) et on place le
//! G27 en liste noire, puis on active le masquage.
//!
//! La fonctionnalité est **adaptative** : si HidHide n'est pas installé, les
//! opérations renvoient [`ErreurHidHide::Indisponible`] et le feeder fonctionne
//! tout de même (le jeu voit alors les deux périphériques).
//!
// « HidHide » est un nom de produit, pas un identifiant de code : on évite que
// clippy réclame des backticks tout au long du module.
#![allow(clippy::doc_markdown)]

#[cfg(windows)]
mod controle;

/// Aide affichée quand HidHide est indisponible (CLI et GUI).
pub const AIDE_HIDHIDE: &str = "HidHide est introuvable ou inactif. Pour cacher le volant réel au jeu (et éviter les doubles entrées), installez HidHide (x64) depuis https://github.com/nefarius/HidHide/releases.\nSans HidHide, le feeder vJoy fonctionne, mais le jeu peut voir à la fois le G27 et le device vJoy.";

/// Erreur d'une opération HidHide.
#[derive(Debug, thiserror::Error)]
pub enum ErreurHidHide {
    /// HidHide n'est pas installé, ou son pilote est inactif.
    #[error("HidHide est indisponible (non installé ou pilote inactif)")]
    Indisponible,
    /// Aucun G27 en mode natif à masquer.
    #[error("aucun G27 en mode natif à masquer")]
    G27Introuvable,
    /// Le chemin d'instance du G27 n'a pas pu être déterminé.
    #[error("chemin d'instance du G27 illisible")]
    InstanceIllisible,
    /// Échec d'un appel système HidHide.
    #[error("erreur d'accès à HidHide : {0}")]
    Io(String),
}

/// Indique si le pilote HidHide est présent et pilotable.
#[must_use]
pub fn disponible() -> bool {
    #[cfg(windows)]
    {
        controle::disponible()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Masque le G27 natif au jeu : liste blanche = notre exe, liste noire = le G27,
/// puis active le masquage.
///
/// # Errors
///
/// [`ErreurHidHide::Indisponible`] si HidHide n'est pas pilotable,
/// [`ErreurHidHide::G27Introuvable`] / [`ErreurHidHide::InstanceIllisible`] si le
/// volant ne peut être identifié, ou [`ErreurHidHide::Io`] sur échec d'appel.
pub fn masquer_g27(api: &hidapi::HidApi) -> Result<(), ErreurHidHide> {
    #[cfg(windows)]
    {
        controle::masquer_g27(api)
    }
    #[cfg(not(windows))]
    {
        let _ = api;
        Err(ErreurHidHide::Indisponible)
    }
}

/// Désactive le masquage (le G27 redevient visible de toutes les applications).
///
/// # Errors
///
/// [`ErreurHidHide::Indisponible`] si HidHide n'est pas pilotable, ou
/// [`ErreurHidHide::Io`] en cas d'échec d'appel système.
pub fn demasquer() -> Result<(), ErreurHidHide> {
    #[cfg(windows)]
    {
        controle::demasquer()
    }
    #[cfg(not(windows))]
    {
        Err(ErreurHidHide::Indisponible)
    }
}

/// Déduit le **chemin d'instance** d'un périphérique (`HID\VID_…\…`) à partir de
/// son **chemin d'interface** hidapi (`\\?\HID#VID_…#…#{guid}`).
///
/// Transformation : on retire le préfixe `\\?\` et le suffixe `#{guid}`, on
/// remplace les `#` par des `\`, et on met en majuscules (convention des
/// identifiants d'instance Windows, comparés sans tenir compte de la casse).
#[must_use]
pub fn instance_depuis_interface(interface: &str) -> Option<String> {
    let sans_prefixe = interface.strip_prefix(r"\\?\").unwrap_or(interface);
    let sans_guid = sans_prefixe.split("#{").next().unwrap_or(sans_prefixe);
    let instance = sans_guid.replace('#', "\\").to_uppercase();
    (!instance.is_empty()).then_some(instance)
}

#[cfg(test)]
mod tests {
    use super::instance_depuis_interface;

    #[test]
    fn deduit_le_chemin_d_instance() {
        let interface =
            r"\\?\HID#VID_046D&PID_C29B#7&abcd1234&0&0000#{4d1e55b2-f3b9-4974-a76e-7c7a73e9b1d8}";
        assert_eq!(
            instance_depuis_interface(interface).unwrap(),
            r"HID\VID_046D&PID_C29B\7&ABCD1234&0&0000"
        );
    }

    #[test]
    fn tolere_l_absence_de_prefixe_et_de_guid() {
        assert_eq!(
            instance_depuis_interface("hid#vid_046d&pid_c29b#abc").unwrap(),
            r"HID\VID_046D&PID_C29B\ABC"
        );
        assert!(instance_depuis_interface("").is_none());
    }
}
