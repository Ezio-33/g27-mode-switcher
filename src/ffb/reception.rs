//! Réception des paquets FFB de vJoy : enregistre le callback générique, **parse**
//! chaque paquet en [`MessageFfb`] (dans le trampoline) et l'achemine vers un canal.
//!
//! ⚠️ **Sûreté FFI** : le trampoline est appelé par le thread interne de vJoy. Il ne
//! doit **jamais** paniquer à travers la frontière C (UB) : tout est encapsulé dans
//! `catch_unwind` et l'échec d'envoi (receiver fermé) est ignoré. Il reste minimal :
//! parser + envoyer.
//!
//! ⚠️ **Cycle de vie** : le SDK vJoy n'offre **pas** de désenregistrement. Le contexte
//! (le `Sender` boxé) doit rester valide tant que le device est acquis ; `RecepteurFfb`
//! doit donc être **droppé après le `RelinquishVJD`** du device. À son `Drop`, on
//! remplace d'abord le callback par un no-op (userdata nul) **puis** on libère le
//! contexte.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::Sender;

use crate::vjoy::{DonneesFfb, Vjoy};

use super::analyse::analyser;
use super::message::MessageFfb;

/// Contexte passé comme `userdata` au callback : de quoi parser (le `Vjoy`) et
/// acheminer le résultat (le `Sender`).
struct ContexteFfb {
    vjoy: &'static Vjoy,
    sender: Sender<MessageFfb>,
}

/// Récepteur FFB actif : possède le contexte boxé (userdata du callback). Le device
/// vJoy doit déjà être acquis ; cf. le contrat de cycle de vie en tête de module.
pub struct RecepteurFfb {
    vjoy: &'static Vjoy,
    /// Contexte boxé, passé comme userdata au callback C. Libéré uniquement au `Drop`.
    userdata: *mut ContexteFfb,
}

impl RecepteurFfb {
    /// Enregistre le callback FFB sur `vjoy` (device déjà acquis) ; chaque paquet
    /// parsé est transmis sur `sender`. L'appelant garde le `Receiver`.
    #[must_use]
    pub fn enregistrer(vjoy: &'static Vjoy, sender: Sender<MessageFfb>) -> Self {
        let userdata = Box::into_raw(Box::new(ContexteFfb { vjoy, sender }));
        // SAFETY: `userdata` est un `Box` fraîchement créé, conservé vivant dans le
        // champ `userdata` jusqu'au `Drop` (lui-même postérieur au `RelinquishVJD`) ;
        // `trampoline` respecte la convention et ne panique jamais.
        unsafe { vjoy.enregistrer_callback_ffb(trampoline, userdata.cast()) };
        Self { vjoy, userdata }
    }
}

impl Drop for RecepteurFfb {
    fn drop(&mut self) {
        // Détache notre trampoline (callback no-op + userdata nul) AVANT de libérer le
        // contexte : à n'effectuer qu'après le `RelinquishVJD` (les callbacks ont alors
        // cessé) — garanti par l'ordre de déclaration côté appelant.
        // SAFETY: userdata nul (aucune durée de vie à garantir) ; `trampoline_inerte`
        // respecte la convention et ne fait rien.
        unsafe {
            self.vjoy
                .enregistrer_callback_ffb(trampoline_inerte, std::ptr::null_mut());
        }
        // SAFETY: reprend possession du `Box` créé dans `enregistrer` (libéré une
        // seule fois, ici). Plus aucun callback ne référence ce pointeur.
        drop(unsafe { Box::from_raw(self.userdata) });
    }
}

/// Trampoline C appelé par le thread interne FFB de vJoy. **Ne doit jamais paniquer.**
unsafe extern "system" fn trampoline(donnees: *const DonneesFfb, userdata: *mut c_void) {
    // `catch_unwind` : une panique qui traverserait la frontière FFI serait un UB.
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        if donnees.is_null() || userdata.is_null() {
            return;
        }
        // SAFETY: `userdata` est le `*mut ContexteFfb` créé dans `enregistrer`,
        // maintenu vivant tant que le device est acquis. `donnees` pointe une
        // `DonneesFfb` valide fournie par vJoy pour la durée de l'appel.
        let contexte = unsafe { &*(userdata.cast::<ContexteFfb>()) };
        let donnees = unsafe { &*donnees };
        if let Some(message) = analyser(contexte.vjoy, donnees) {
            // `send` : si le receiver est fermé, l'erreur est simplement ignorée.
            let _ = contexte.sender.send(message);
        }
    }));
}

/// Trampoline no-op, installé au `Drop` pour détacher le callback sans rien faire.
unsafe extern "system" fn trampoline_inerte(_donnees: *const DonneesFfb, _userdata: *mut c_void) {}
