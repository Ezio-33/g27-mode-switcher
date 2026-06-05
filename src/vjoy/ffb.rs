//! Liaison FFI à la partie retour de force (FFB) de vJoy.
//!
//! Regroupe la structure de paquet livrée au callback (`FFB_DATA`), le type du
//! callback générique (`FfbRegisterGenCB`) et les helpers de parsing (`Ffb_h_*`) qui
//! extraient les effets d'un paquet. Les structures d'effet sont dans [`super::ffb_effet`].
//!
//! ⚠️ Il n'existe **aucun désenregistrement** du callback : le `userdata` passé doit
//! rester valide tant que le device est acquis (cf. [`crate::ffb`]).
//!
//! ABI sourcée du wrapper officiel `vJoyInterfaceWrap/Wrapper.cs` (pas devinée).

use std::ffi::c_void;

use libloading::Library;

use super::ErreurVjoy;
use super::charger_symbole;
use super::ffb_effet::{
    EffetCondition, EffetConstante, EffetEnveloppe, EffetPeriodique, EffetRampe, OperationFfb,
    RapportEffet,
};

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
type FnEnregistrer = unsafe extern "C" fn(CallbackFfb, *mut c_void);

/// Signature commune des helpers `Ffb_h_*` : `(packet, &mut sortie) -> statut`
/// (`0` = succès). `T` est le type extrait (entier ou structure d'effet).
type FnHelper<T> = unsafe extern "C" fn(*const DonneesFfb, *mut T) -> u32;

/// Pointeurs de fonctions FFB résolus depuis `vJoyInterface.dll`.
pub(super) struct FfbHelpers {
    register: FnEnregistrer,
    h_type: FnHelper<i32>,
    h_ebi: FnHelper<i32>,
    h_new: FnHelper<i32>,
    h_ctrl: FnHelper<i32>,
    h_gain: FnHelper<u8>,
    h_report: FnHelper<RapportEffet>,
    h_constant: FnHelper<EffetConstante>,
    h_period: FnHelper<EffetPeriodique>,
    h_cond: FnHelper<EffetCondition>,
    h_ramp: FnHelper<EffetRampe>,
    h_envlp: FnHelper<EffetEnveloppe>,
    h_op: FnHelper<OperationFfb>,
}

impl FfbHelpers {
    /// Résout tous les symboles FFB de la DLL déjà chargée.
    pub(super) fn charger(lib: &Library) -> Result<Self, ErreurVjoy> {
        Ok(Self {
            register: *charger_symbole(lib, b"FfbRegisterGenCB\0")?,
            h_type: *charger_symbole(lib, b"Ffb_h_Type\0")?,
            h_ebi: *charger_symbole(lib, b"Ffb_h_EBI\0")?,
            h_new: *charger_symbole(lib, b"Ffb_h_EffNew\0")?,
            h_ctrl: *charger_symbole(lib, b"Ffb_h_DevCtrl\0")?,
            h_gain: *charger_symbole(lib, b"Ffb_h_DevGain\0")?,
            h_report: *charger_symbole(lib, b"Ffb_h_Eff_Report\0")?,
            h_constant: *charger_symbole(lib, b"Ffb_h_Eff_Constant\0")?,
            h_period: *charger_symbole(lib, b"Ffb_h_Eff_Period\0")?,
            h_cond: *charger_symbole(lib, b"Ffb_h_Eff_Cond\0")?,
            h_ramp: *charger_symbole(lib, b"Ffb_h_Eff_Ramp\0")?,
            h_envlp: *charger_symbole(lib, b"Ffb_h_Eff_Envlp\0")?,
            h_op: *charger_symbole(lib, b"Ffb_h_EffOp\0")?,
        })
    }
}

/// Appelle un helper `Ffb_h_*` et renvoie la sortie si le statut vaut `0`.
fn lire<T: Default>(helper: FnHelper<T>, donnees: &DonneesFfb) -> Option<T> {
    let mut sortie = T::default();
    // SAFETY: `helper` est un pointeur `Ffb_h_*` valide chargé de la DLL ; `donnees`
    // pointe une `DonneesFfb` valide (le callback est en cours) ; `sortie` est un
    // buffer de la taille exacte de la structure attendue (offsets et tailles
    // verrouillés par les tests de `super::ffb_effet`). `0` = succès.
    let statut = unsafe { helper(std::ptr::from_ref(donnees), &raw mut sortie) };
    (statut == 0).then_some(sortie)
}

impl super::Vjoy {
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
        unsafe { (self.ffb.register)(callback, userdata) };
    }

    /// Type de paquet FFB (`FFBPType`), ou `None` si non applicable.
    #[must_use]
    pub fn ffb_type(&self, donnees: &DonneesFfb) -> Option<i32> {
        lire(self.ffb.h_type, donnees)
    }

    /// Index de bloc d'effet (`Effect Block Index`) du paquet.
    #[must_use]
    pub fn ffb_bloc(&self, donnees: &DonneesFfb) -> Option<i32> {
        lire(self.ffb.h_ebi, donnees)
    }

    /// Type du nouvel effet créé (`FFBEType`).
    #[must_use]
    pub fn ffb_nouvel_effet(&self, donnees: &DonneesFfb) -> Option<i32> {
        lire(self.ffb.h_new, donnees)
    }

    /// Code de contrôle device (`FFB_CTRL`).
    #[must_use]
    pub fn ffb_controle(&self, donnees: &DonneesFfb) -> Option<i32> {
        lire(self.ffb.h_ctrl, donnees)
    }

    /// Gain global du device (0–255).
    #[must_use]
    pub fn ffb_gain(&self, donnees: &DonneesFfb) -> Option<u8> {
        lire(self.ffb.h_gain, donnees)
    }

    /// Paramètres généraux d'un effet (`FFB_EFF_REPORT`).
    #[must_use]
    pub fn ffb_rapport(&self, donnees: &DonneesFfb) -> Option<RapportEffet> {
        lire(self.ffb.h_report, donnees)
    }

    /// Force constante (`FFB_EFF_CONSTANT`).
    #[must_use]
    pub fn ffb_constante(&self, donnees: &DonneesFfb) -> Option<EffetConstante> {
        lire(self.ffb.h_constant, donnees)
    }

    /// Effet périodique (`FFB_EFF_PERIOD`).
    #[must_use]
    pub fn ffb_periodique(&self, donnees: &DonneesFfb) -> Option<EffetPeriodique> {
        lire(self.ffb.h_period, donnees)
    }

    /// Effet conditionnel (`FFB_EFF_COND`).
    #[must_use]
    pub fn ffb_condition(&self, donnees: &DonneesFfb) -> Option<EffetCondition> {
        lire(self.ffb.h_cond, donnees)
    }

    /// Force en rampe (`FFB_EFF_RAMP`).
    #[must_use]
    pub fn ffb_rampe(&self, donnees: &DonneesFfb) -> Option<EffetRampe> {
        lire(self.ffb.h_ramp, donnees)
    }

    /// Enveloppe d'un effet (`FFB_EFF_ENVLP`).
    #[must_use]
    pub fn ffb_enveloppe(&self, donnees: &DonneesFfb) -> Option<EffetEnveloppe> {
        lire(self.ffb.h_envlp, donnees)
    }

    /// Opération sur un effet (`FFB_EFF_OP`).
    #[must_use]
    pub fn ffb_operation(&self, donnees: &DonneesFfb) -> Option<OperationFfb> {
        lire(self.ffb.h_op, donnees)
    }
}
