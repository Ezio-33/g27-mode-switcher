//! Liaison FFI à la partie retour de force (FFB) de vJoy.
//!
//! Définit la structure de paquet livrée au callback (`FFB_DATA` du SDK vJoy) et le
//! type du callback générique enregistré via `FfbRegisterGenCB`. Le parsing des
//! effets (`Ffb_h_*`) viendra à l'étape suivante (Phase 5, commit 2).
//!
//! ⚠️ Il n'existe **aucun désenregistrement** de ce callback dans le SDK vJoy : le
//! `userdata` passé doit rester valide tant que le device est acquis (les callbacks
//! cessent au `RelinquishVJD`).
//!
//! ABI sourcée du wrapper officiel `vJoyInterfaceWrap/Wrapper.cs` (pas devinée).

use std::ffi::c_void;

use super::Vjoy;

/// Données FFB livrées au callback (`FFB_DATA`). `data` pointe vers le rapport HID
/// brut ; `size`/`cmd` sont lisibles directement (sans déréférencer `data`).
#[repr(C)]
pub struct DonneesFfb {
    /// Taille du paquet (champ `size` du SDK).
    pub size: u32,
    /// Commande / type de rapport (champ `cmd` du SDK).
    pub cmd: u32,
    /// Pointeur vers le rapport HID brut (valide uniquement pendant l'appel).
    pub data: *const u8,
}

/// Callback générique FFB. La convention SDK est `CALLBACK` (`__stdcall`), identique
/// à C sur x64 — d'où `extern "system"`. `userdata` est repassé tel quel.
pub type CallbackFfb = unsafe extern "system" fn(donnees: *const DonneesFfb, userdata: *mut c_void);

/// Signature C de `FfbRegisterGenCB` (`__cdecl`).
pub(super) type FnEnregistrerFfb = unsafe extern "C" fn(CallbackFfb, *mut c_void);

impl Vjoy {
    /// Enregistre (ou remplace) le callback générique FFB de vJoy.
    ///
    /// # Safety
    ///
    /// `userdata` est repassé tel quel au `callback` ; l'appelant doit garantir qu'il
    /// **reste valide tant que le device est acquis** (le SDK vJoy n'offre pas de
    /// désenregistrement ; les callbacks cessent au `RelinquishVJD`). `callback` doit
    /// respecter la convention et ne jamais paniquer. Cf. [`crate::ffb`].
    pub unsafe fn enregistrer_callback_ffb(&self, callback: CallbackFfb, userdata: *mut c_void) {
        // SAFETY: appel C `__cdecl` respectant la signature ; les invariants sur la
        // durée de vie de `userdata` sont délégués à l'appelant (contrat ci-dessus).
        unsafe { (self.ffb_register)(callback, userdata) };
    }
}
