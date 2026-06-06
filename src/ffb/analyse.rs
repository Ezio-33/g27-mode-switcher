//! Analyse d'un paquet FFB brut (`DonneesFfb`) en [`MessageFfb`] neutre.
//!
//! Appelé **depuis le trampoline** (thread FFB de vJoy), pendant que le pointeur du
//! paquet est valide : on lit le type de paquet puis on extrait l'effet correspondant
//! via les helpers `Ffb_h_*`. Les codes de type de paquet viennent de `FFBPType`.

use crate::vjoy::{DonneesFfb, Vjoy};

use super::message::{ControleDevice, MessageFfb, OperationEffet, TypeEffet};

// Types de paquet FFB (`FFBPType`), sourcés du SDK vJoy.
const PT_EFFREP: i32 = 0x01; // Set Effect Report
const PT_ENVREP: i32 = 0x02; // Set Envelope Report
const PT_CONDREP: i32 = 0x03; // Set Condition Report
const PT_PRIDREP: i32 = 0x04; // Set Periodic Report
const PT_CONSTREP: i32 = 0x05; // Set Constant Force Report
const PT_RAMPREP: i32 = 0x06; // Set Ramp Force Report
const PT_EFOPREP: i32 = 0x0A; // Effect Operation Report
const PT_CTRLREP: i32 = 0x0C; // Device Control
const PT_GAINREP: i32 = 0x0D; // Device Gain Report
const PT_NEWEFREP: i32 = 0x11; // Create New Effect Report

/// Décode un paquet FFB en message neutre, ou `None` si le type est non géré.
// Dispatch 1-vers-1 sur le type de paquet : le découper en sous-fonctions ajouterait
// de l'indirection sans clarté.
#[allow(clippy::too_many_lines)]
pub(super) fn analyser(vjoy: &Vjoy, donnees: &DonneesFfb) -> Option<MessageFfb> {
    let message = match vjoy.ffb_type(donnees)? {
        PT_CONSTREP => {
            let e = vjoy.ffb_constante(donnees)?;
            MessageFfb::Constante {
                bloc: e.bloc,
                magnitude: signe_16(e.magnitude),
            }
        }
        PT_RAMPREP => {
            let e = vjoy.ffb_rampe(donnees)?;
            MessageFfb::Rampe {
                bloc: e.bloc,
                debut: signe_16(e.debut),
                fin: signe_16(e.fin),
            }
        }
        PT_PRIDREP => {
            let e = vjoy.ffb_periodique(donnees)?;
            MessageFfb::Periodique {
                bloc: e.bloc,
                magnitude: e.magnitude,
                offset: signe_16(e.offset),
                phase: e.phase,
                periode: e.periode,
            }
        }
        PT_CONDREP => {
            let e = vjoy.ffb_condition(donnees)?;
            MessageFfb::Condition {
                bloc: e.bloc,
                centre: signe_16(e.centre),
                coeff_pos: signe_16(e.coeff_pos),
                coeff_neg: signe_16(e.coeff_neg),
                satur_pos: e.satur_pos,
                satur_neg: e.satur_neg,
                deadband: e.deadband,
            }
        }
        PT_ENVREP => {
            let e = vjoy.ffb_enveloppe(donnees)?;
            MessageFfb::Enveloppe {
                bloc: e.bloc,
                niveau_attaque: e.niveau_attaque,
                niveau_fondu: e.niveau_fondu,
                temps_attaque: e.temps_attaque,
                temps_fondu: e.temps_fondu,
            }
        }
        PT_EFFREP => {
            let e = vjoy.ffb_rapport(donnees)?;
            MessageFfb::Rapport {
                bloc: e.bloc,
                type_effet: TypeEffet::depuis_code(e.type_effet),
                duree_ms: e.duree,
                gain: e.gain,
                direction: e.direction,
            }
        }
        PT_EFOPREP => {
            let e = vjoy.ffb_operation(donnees)?;
            MessageFfb::Operation {
                bloc: e.bloc,
                operation: OperationEffet::depuis_code(e.operation),
                repetitions: e.repetitions,
            }
        }
        PT_CTRLREP => {
            MessageFfb::Controle(ControleDevice::depuis_code(vjoy.ffb_controle(donnees)?))
        }
        PT_GAINREP => MessageFfb::Gain(vjoy.ffb_gain(donnees)?),
        PT_NEWEFREP => {
            let type_code = vjoy.ffb_nouvel_effet(donnees)?;
            let bloc = u8::try_from(vjoy.ffb_bloc(donnees).unwrap_or(0)).unwrap_or(0);
            MessageFfb::NouvelEffet {
                bloc,
                type_effet: TypeEffet::depuis_code(type_code),
            }
        }
        _ => return None,
    };
    Some(message)
}

/// Réinterprète les 16 bits de poids faible comme un entier **signé**.
///
/// Les helpers `Ffb_h_*` écrivent les magnitudes/coefficients signés sur 16 bits, les
/// octets de poids fort restant à 0. Lus tels quels, une valeur négative (force vers la
/// gauche) apparaîtrait comme un grand positif (ex. `0xF800` = 63488 au lieu de −2048).
/// On tronque donc aux 16 bits de poids faible avant de réinterpréter le signe.
#[allow(clippy::cast_possible_truncation)]
fn signe_16(valeur: i32) -> i32 {
    i32::from(valeur as i16)
}
