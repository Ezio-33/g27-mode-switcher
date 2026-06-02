//! Lecture des rapports d'entrée du G27 natif via hidapi (non bloquante).
//!
//! Ouvre l'interface HID du G27 en mode natif et lit ses rapports d'entrée bruts.
//! La lecture est **non exclusive** : le jeu (ou un device vJoy alimenté) continue
//! de voir le volant en parallèle. Le décodage des rapports (boutons mappables
//! côté `keymapper`, entrées complètes côté feeder vJoy) est laissé à l'appelant.

use hidapi::{HidApi, HidDevice};

use crate::hid::{self, NativeLookup};

/// Taille maximale d'un rapport d'entrée lu (octets).
const TAILLE_RAPPORT: usize = 16;

/// Erreurs de lecture des entrées du G27.
#[derive(Debug, thiserror::Error)]
pub enum ErreurLecture {
    /// Aucun G27 détecté.
    #[error("aucun G27 détecté")]
    NoG27,
    /// Un G27 est présent mais en mode compatibilité.
    #[error("le G27 est en mode compatibilité ; basculez d'abord en mode natif")]
    NotNative,
    /// Échec d'ouverture du périphérique HID.
    #[error("ouverture du périphérique HID impossible : {0}")]
    Ouverture(hidapi::HidError),
    /// Échec de lecture d'un rapport HID.
    #[error("lecture HID impossible : {0}")]
    Lecture(hidapi::HidError),
}

/// Lecteur des rapports d'entrée du G27 natif.
pub struct LecteurG27 {
    device: HidDevice,
    tampon: [u8; TAILLE_RAPPORT],
    longueur: usize,
}

impl LecteurG27 {
    /// Ouvre le G27 natif pour lecture.
    ///
    /// # Errors
    ///
    /// [`ErreurLecture::NoG27`] / [`ErreurLecture::NotNative`] selon l'état
    /// détecté, ou [`ErreurLecture::Ouverture`] si l'ouverture HID échoue.
    pub fn ouvrir(api: &HidApi) -> Result<Self, ErreurLecture> {
        let info = hid::find_native_g27(api).map_err(|manque| match manque {
            NativeLookup::NotNative => ErreurLecture::NotNative,
            NativeLookup::NoG27 => ErreurLecture::NoG27,
        })?;
        let device = api
            .open_path(info.path.as_c_str())
            .map_err(ErreurLecture::Ouverture)?;
        Ok(Self {
            device,
            tampon: [0; TAILLE_RAPPORT],
            longueur: 0,
        })
    }

    /// Attend (jusqu'à `delai_ms`) un rapport et le mémorise.
    ///
    /// Renvoie `true` si un nouveau rapport a été lu, `false` en cas de délai
    /// écoulé sans rapport.
    ///
    /// # Errors
    ///
    /// [`ErreurLecture::Lecture`] si la lecture HID échoue.
    pub fn lire(&mut self, delai_ms: i32) -> Result<bool, ErreurLecture> {
        let lus = self
            .device
            .read_timeout(&mut self.tampon, delai_ms)
            .map_err(ErreurLecture::Lecture)?;
        self.longueur = lus;
        Ok(lus > 0)
    }

    /// Dernier rapport brut lu (octets effectivement reçus).
    #[must_use]
    pub fn rapport(&self) -> &[u8] {
        &self.tampon[..self.longueur]
    }
}
