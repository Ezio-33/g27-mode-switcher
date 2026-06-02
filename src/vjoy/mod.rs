//! Liaison dynamique à `vJoyInterface.dll` (chargée à l'exécution via libloading).
//!
//! L'application est **adaptative** : si vJoy n'est pas installé (DLL absente),
//! [`Vjoy::charger`] échoue proprement et la fonctionnalité FFB reste désactivée,
//! sans empêcher le reste de fonctionner. Le chargement dynamique évite toute
//! liaison statique au SDK vJoy.
//!
//! Ce module est l'unique frontière FFI vers vJoy : chaque bloc `unsafe` y est
//! documenté par un commentaire `// SAFETY:`. La DLL ne se charge réellement que
//! sous Windows avec vJoy installé ; ailleurs, le chargement renvoie une erreur.
#![allow(unsafe_code)]

mod decouverte;
mod position;
#[cfg(windows)]
mod registre;

use std::path::PathBuf;

pub use decouverte::{Recherche, rechercher_dll};
pub use position::JoystickPositionV2;

use libloading::{Library, Symbol};

/// Aide affichée quand `vJoyInterface.dll` est introuvable (CLI et GUI).
pub const AIDE_VJOY_INTROUVABLE: &str = "vJoyInterface.dll introuvable. Le retour de force nécessite vJoy (version x64).\nEmplacement attendu : C:\\Program Files\\vJoy\\x64\\vJoyInterface.dll\nInstallez vJoy (x64), ou copiez vJoyInterface.dll à côté de cet exécutable.";

/// Signatures C des fonctions vJoy utilisées (ABI C de la plateforme).
type FnDrapeau = unsafe extern "C" fn() -> i32;
type FnVersion = unsafe extern "C" fn() -> i16;
type FnDevice = unsafe extern "C" fn(u32) -> i32;
type FnLiberer = unsafe extern "C" fn(u32);
type FnMaj = unsafe extern "C" fn(u32, *mut JoystickPositionV2) -> i32;

/// Erreur de liaison à vJoy.
#[derive(Debug, thiserror::Error)]
pub enum ErreurVjoy {
    /// La DLL vJoy n'a été trouvée à aucun emplacement (message pédagogique
    /// complet, suivi de la liste des chemins testés).
    #[error("{0}")]
    Introuvable(String),
    /// La DLL a été trouvée mais n'a pas pu être chargée.
    #[error("vJoyInterface.dll trouvée mais illisible : {0}")]
    Chargement(libloading::Error),
    /// Un symbole attendu est absent de la DLL.
    #[error("symbole vJoy manquant : {0}")]
    Symbole(libloading::Error),
}

/// Compose l'erreur « introuvable » : aide pédagogique + chemins testés.
fn erreur_introuvable(testes: &[PathBuf]) -> ErreurVjoy {
    let mut message = String::from(AIDE_VJOY_INTROUVABLE);
    message.push_str("\nChemins testés :");
    for chemin in testes {
        message.push_str("\n  - ");
        message.push_str(&chemin.display().to_string());
    }
    ErreurVjoy::Introuvable(message)
}

/// Statut d'un device vJoy (`GetVJDStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatutVjd {
    /// Device possédé par cette application.
    Possede,
    /// Device libre (acquérable).
    Libre,
    /// Device occupé par une autre application.
    Occupe,
    /// Device absent (non configuré dans vJoy).
    Absent,
    /// Statut inconnu.
    Inconnu,
}

impl StatutVjd {
    /// Convertit le code `VjdStat` du SDK en variante.
    fn depuis_code(code: i32) -> Self {
        match code {
            0 => Self::Possede,
            1 => Self::Libre,
            2 => Self::Occupe,
            3 => Self::Absent,
            _ => Self::Inconnu,
        }
    }
}

/// Interface vers le pilote vJoy (DLL chargée + symboles résolus).
pub struct Vjoy {
    // La bibliothèque doit rester vivante tant que les pointeurs de fonction
    // ci-dessous sont utilisés (ils pointent dans son image mémoire).
    _lib: Library,
    enabled: FnDrapeau,
    version: FnVersion,
    status: FnDevice,
    acquire: FnDevice,
    relinquish: FnLiberer,
    update: FnMaj,
}

impl Vjoy {
    /// Recherche `vJoyInterface.dll` (cf. [`rechercher_dll`]) puis la charge et
    /// résout les symboles nécessaires.
    ///
    /// # Errors
    ///
    /// [`ErreurVjoy::Introuvable`] si la DLL n'est trouvée nulle part (avec aide
    /// et chemins testés), [`ErreurVjoy::Chargement`] si elle est illisible,
    /// [`ErreurVjoy::Symbole`] si un symbole attendu manque.
    pub fn charger() -> Result<Self, ErreurVjoy> {
        let recherche = rechercher_dll();
        let Some(chemin) = recherche.trouve else {
            return Err(erreur_introuvable(&recherche.testes));
        };
        // SAFETY: chargement d'une DLL par chemin complet résolu ; libloading
        // renvoie une erreur si elle est invalide (pas de comportement indéfini).
        let lib = unsafe { Library::new(&chemin) }.map_err(ErreurVjoy::Chargement)?;

        let enabled = *charger_symbole::<FnDrapeau>(&lib, b"vJoyEnabled\0")?;
        let version = *charger_symbole::<FnVersion>(&lib, b"GetvJoyVersion\0")?;
        let status = *charger_symbole::<FnDevice>(&lib, b"GetVJDStatus\0")?;
        let acquire = *charger_symbole::<FnDevice>(&lib, b"AcquireVJD\0")?;
        let relinquish = *charger_symbole::<FnLiberer>(&lib, b"RelinquishVJD\0")?;
        let update = *charger_symbole::<FnMaj>(&lib, b"UpdateVJD\0")?;

        Ok(Self {
            _lib: lib,
            enabled,
            version,
            status,
            acquire,
            relinquish,
            update,
        })
    }

    /// Indique si le pilote vJoy est activé.
    #[must_use]
    pub fn active(&self) -> bool {
        // SAFETY: fonction sans paramètre du SDK vJoy ; signature C respectée.
        unsafe { (self.enabled)() != 0 }
    }

    /// Version du pilote vJoy (format SDK).
    #[must_use]
    pub fn version(&self) -> i16 {
        // SAFETY: fonction sans paramètre du SDK vJoy ; signature C respectée.
        unsafe { (self.version)() }
    }

    /// Statut du device vJoy `id`.
    #[must_use]
    pub fn statut(&self, id: u32) -> StatutVjd {
        // SAFETY: appel C avec un `UINT` ; renvoie un code `VjdStat`.
        StatutVjd::depuis_code(unsafe { (self.status)(id) })
    }

    /// Acquiert le device vJoy `id`. Renvoie `true` en cas de succès.
    #[must_use]
    pub fn acquerir(&self, id: u32) -> bool {
        // SAFETY: appel C avec un `UINT` ; renvoie un `BOOL`.
        unsafe { (self.acquire)(id) != 0 }
    }

    /// Libère le device vJoy `id`.
    pub fn liberer(&self, id: u32) {
        // SAFETY: appel C avec un `UINT` ; ne renvoie rien.
        unsafe { (self.relinquish)(id) };
    }

    /// Pousse l'état complet `position` vers le device `id`. Renvoie `true` si
    /// la mise à jour a réussi.
    #[must_use]
    pub fn mettre_a_jour(&self, id: u32, position: &mut JoystickPositionV2) -> bool {
        position.device = u8::try_from(id).unwrap_or_default();
        // SAFETY: `position` est un pointeur valide vers une structure dont la
        // disposition correspond à `JOYSTICK_POSITION_V2` (cf. test de taille).
        unsafe { (self.update)(id, std::ptr::from_mut(position)) != 0 }
    }
}

/// Résout un symbole de la DLL en pointeur de fonction typé.
fn charger_symbole<'lib, T>(lib: &'lib Library, nom: &[u8]) -> Result<Symbol<'lib, T>, ErreurVjoy> {
    // SAFETY: `nom` désigne une fonction exportée du SDK vJoy dont la signature
    // `T` correspond à la déclaration officielle ; un symbole absent est remonté
    // comme erreur, sans déréférencement.
    unsafe { lib.get::<T>(nom) }.map_err(ErreurVjoy::Symbole)
}

#[cfg(test)]
mod tests {
    use super::StatutVjd;

    #[test]
    fn codes_de_statut_mappes() {
        assert_eq!(StatutVjd::depuis_code(0), StatutVjd::Possede);
        assert_eq!(StatutVjd::depuis_code(1), StatutVjd::Libre);
        assert_eq!(StatutVjd::depuis_code(2), StatutVjd::Occupe);
        assert_eq!(StatutVjd::depuis_code(3), StatutVjd::Absent);
        assert_eq!(StatutVjd::depuis_code(42), StatutVjd::Inconnu);
    }
}
