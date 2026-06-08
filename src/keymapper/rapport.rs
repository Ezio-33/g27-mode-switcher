//! Extraction de l'état des boutons mappables depuis un rapport HID brut.

use super::{Bouton, EtatBoutons};

/// Octet du rapport hidapi où commence le champ de boutons (1-indexés).
///
/// ⚠️ Valeur **provisoire**, à confirmer sur matériel via la commande `boutons` :
/// le bouton HID n° `k` est lu au bit `(k - 1)` compté à partir de cet octet.
/// Le format du rapport du G27 natif n'étant pas testable sans volant, la
/// commande de debug affiche les octets bruts et les bits armés pour caler
/// précisément cet offset.
pub const OCTET_DEBUT_BOUTONS: usize = 0;

/// Construit un [`EtatBoutons`] à partir d'un rapport HID brut.
///
/// Le bouton HID n° `k` (cf. [`Bouton::numero_g27`]) est lu au bit `(k - 1)`
/// compté depuis [`OCTET_DEBUT_BOUTONS`]. Les bits situés au-delà du rapport
/// fourni sont considérés comme relâchés.
#[must_use]
pub fn boutons_depuis_rapport(rapport: &[u8]) -> EtatBoutons {
    let mut etat = EtatBoutons::vide();
    for bouton in Bouton::TOUS {
        if bit_arme(rapport, bouton.numero_g27()) {
            etat.definir(bouton, true);
        }
    }
    etat
}

/// Indique si le bouton HID 1-indexé `numero` est armé dans le rapport.
fn bit_arme(rapport: &[u8], numero: u8) -> bool {
    let index = usize::from(numero).saturating_sub(1); // passage en 0-indexé
    let octet = OCTET_DEBUT_BOUTONS + index / 8;
    let bit = index % 8;
    rapport
        .get(octet)
        .is_some_and(|valeur| valeur & (1 << bit) != 0)
}

#[cfg(test)]
mod tests {
    use super::super::Bouton;
    use super::boutons_depuis_rapport;

    #[test]
    fn lit_rapports_et_marche_arriere() {
        // Avec OCTET_DEBUT_BOUTONS = 0 : bouton 13 → octet 1 bit 4 ;
        // bouton 15 → octet 1 bit 6 ; bouton 23 → octet 2 bit 6.
        let mut rapport = [0u8; 4];
        rapport[1] = (1 << 4) | (1 << 6); // rapports 1 et 3
        rapport[2] = 1 << 6; // marche arrière
        let etat = boutons_depuis_rapport(&rapport);
        assert!(etat.contient(Bouton::Rapport1));
        assert!(etat.contient(Bouton::Rapport3));
        assert!(etat.contient(Bouton::MarcheArriere));
        assert!(!etat.contient(Bouton::Rapport2));
        assert!(!etat.contient(Bouton::Rapport6));
    }

    #[test]
    fn rapport_trop_court_tout_relache() {
        let etat = boutons_depuis_rapport(&[0xFF]);
        // Seuls les boutons 1–8 tiendraient dans l'octet 0 ; aucun rapport de
        // boîte H (13+) n'y figure.
        for bouton in Bouton::TOUS {
            assert!(!etat.contient(bouton));
        }
    }
}
