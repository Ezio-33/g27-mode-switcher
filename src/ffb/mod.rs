//! Pont retour de force (FFB) — Phase 5.
//!
//! Reçoit les effets que le jeu envoie au device vJoy, les traduira en commandes de
//! force Logitech pour le G27 réel. Pour l'instant (commit 1) : **réception brute**
//! des paquets, pour valider le canal et l'initialisation de la fenêtre FFB de vJoy
//! sur le thread worker (sans regel ni popup).
//!
//! « vJoy »/« FFB » sont des noms de produits/techniques, pas des identifiants.
#![allow(clippy::doc_markdown)]

mod reception;

pub use reception::{PaquetFfb, RecepteurFfb};

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::feeder::{DeviceVjoyAcquis, ErreurFeeder, preparer_vjoy};

/// Pause de la boucle de capture entre deux drainages du canal (ms).
const DELAI_DRAIN_MS: u64 = 20;

/// Acquiert le device vJoy `id`, enregistre le récepteur FFB et appelle `sur_paquet`
/// pour chaque paquet reçu, jusqu'à ce que `arret` passe à `true`.
///
/// Diagnostic uniquement (pas de masquage) : sert à vérifier que les effets du jeu
/// arrivent et que la fenêtre FFB de vJoy s'initialise sur ce thread. **À appeler hors
/// du thread GUI** (acquisition vJoy). Le device doit avoir « Enable Effects » activé.
///
/// # Errors
///
/// [`ErreurFeeder`] si l'acquisition du device vJoy échoue.
pub fn capturer(
    id: u32,
    arret: &AtomicBool,
    mut sur_paquet: impl FnMut(PaquetFfb),
) -> Result<(), ErreurFeeder> {
    let vjoy = preparer_vjoy(id)?;
    // ⚠️ Ordre de déclaration = ordre de `Drop` inverse : `_recepteur` déclaré en
    // premier ⇒ droppé en DERNIER (libère le userdata) ; `_garde` déclaré ensuite ⇒
    // droppé en PREMIER (RelinquishVJD). On relâche donc le device AVANT de libérer le
    // userdata, comme l'exige le contrat de `RecepteurFfb`.
    let (_recepteur, paquets) = RecepteurFfb::enregistrer(vjoy);
    let _garde = DeviceVjoyAcquis::new(vjoy, id);

    while !arret.load(Ordering::Relaxed) {
        while let Ok(paquet) = paquets.try_recv() {
            sur_paquet(paquet);
        }
        thread::sleep(Duration::from_millis(DELAI_DRAIN_MS));
    }
    Ok(())
}
