//! Injection de touches clavier depuis les entrées du G27 (D-pad + boutons d'action).
//!
//! Certains jeux (Forza) ne naviguent leurs menus/map qu'avec un périphérique qu'ils
//! **reconnaissent** (vrai volant, manette Xbox) ou avec le **clavier** — jamais avec
//! un device vJoy générique. Quand le G27 est masqué (pour que le retour de force passe
//! par vJoy), on traduit donc :
//! - le **D-pad** en flèches ↑↓←→ (navigation) ;
//! - deux **boutons au choix** en **Entrée** (valider) et **Échap** (retour).
//!
//! On n'émet que les **fronts** (appui / relâché) : tenir un contrôle n'envoie pas de
//! répétition (Windows gère l'auto-répétition). Le `Drop` relâche toute touche encore
//! enfoncée pour ne jamais laisser une touche « collée ».

/// Valeur du chapeau quand le D-pad est relâché (cf. [`crate::entree::CHAPEAU_RELACHE`]).
const CHAPEAU_RELACHE: u8 = 8;

/// Scan codes **étendus** des 4 flèches (pavé directionnel).
const SC_UP: u16 = 0x48;
const SC_LEFT: u16 = 0x4B;
const SC_RIGHT: u16 = 0x4D;
const SC_DOWN: u16 = 0x50;
/// Scan codes (non étendus) des touches d'action.
const SC_ENTER: u16 = 0x1C;
const SC_ESCAPE: u16 = 0x01;

/// Flèches : (bit cardinal, scan code). Bits : 1=haut, 2=droite, 4=bas, 8=gauche.
const FLECHES: [(u8, u16); 4] = [(1, SC_UP), (2, SC_RIGHT), (4, SC_DOWN), (8, SC_LEFT)];
/// Actions : (bit, scan code). Bits : 1=valider (Entrée), 2=retour (Échap).
const ACTIONS: [(u8, u16); 2] = [(1, SC_ENTER), (2, SC_ESCAPE)];

/// Traduit les entrées du G27 en frappes clavier (D-pad → flèches, boutons → actions).
pub struct ClavierG27 {
    /// Traduire le D-pad en flèches (sinon les flèches ne sont jamais émises).
    fleches_actives: bool,
    /// Bouton vJoy (1-indexé) déclenchant **Entrée** (`0` = aucun).
    bouton_valider: u8,
    /// Bouton vJoy (1-indexé) déclenchant **Échap** (`0` = aucun).
    bouton_retour: u8,
    /// Bitmask des flèches actuellement enfoncées (1=haut, 2=droite, 4=bas, 8=gauche).
    cardinaux: u8,
    /// Bitmask des actions actuellement enfoncées (1=valider, 2=retour).
    actions: u8,
}

impl ClavierG27 {
    /// Crée l'injecteur. `fleches_actives` traduit le D-pad ; `bouton_valider` /
    /// `bouton_retour` sont des numéros de boutons **vJoy** (`0` = désactivé).
    #[must_use]
    pub fn new(fleches_actives: bool, bouton_valider: u8, bouton_retour: u8) -> Self {
        Self {
            fleches_actives,
            bouton_valider,
            bouton_retour,
            cardinaux: 0,
            actions: 0,
        }
    }

    /// Vrai si l'injecteur a quelque chose à faire (au moins une traduction active).
    #[must_use]
    pub fn utile(&self) -> bool {
        self.fleches_actives || self.bouton_valider != 0 || self.bouton_retour != 0
    }

    /// Met à jour les touches d'après le chapeau et le masque de boutons **vJoy**,
    /// en n'émettant que les fronts (appui/relâché) depuis le dernier appel.
    pub fn appliquer(&mut self, chapeau: u8, boutons_vjoy: u32) {
        let cardinaux = if self.fleches_actives {
            cardinaux(chapeau)
        } else {
            0
        };
        emettre_fronts(&FLECHES, true, &mut self.cardinaux, cardinaux);

        let mut actions = 0u8;
        if bouton_arme(self.bouton_valider, boutons_vjoy) {
            actions |= 1;
        }
        if bouton_arme(self.bouton_retour, boutons_vjoy) {
            actions |= 2;
        }
        emettre_fronts(&ACTIONS, false, &mut self.actions, actions);
    }
}

impl Drop for ClavierG27 {
    fn drop(&mut self) {
        // Relâche toute touche encore enfoncée (sinon elle resterait « collée »).
        emettre_fronts(&FLECHES, true, &mut self.cardinaux, 0);
        emettre_fronts(&ACTIONS, false, &mut self.actions, 0);
    }
}

/// Émet un appui/relâché pour chaque touche dont l'état change entre `etat` et `cible`,
/// puis met `etat` à jour. `etendue` indique des touches du pavé directionnel.
fn emettre_fronts(touches: &[(u8, u16)], etendue: bool, etat: &mut u8, cible: u8) {
    let changes = cible ^ *etat;
    for &(bit, scan) in touches {
        if changes & bit != 0 {
            envoyer(scan, etendue, cible & bit != 0);
        }
    }
    *etat = cible;
}

/// Vrai si le bouton vJoy `numero` (1-indexé, `0` = désactivé) est armé dans `masque`.
/// Borné à 32 (largeur du masque) pour éviter tout décalage de bits hors plage.
fn bouton_arme(numero: u8, masque: u32) -> bool {
    (1..=32).contains(&numero) && masque & (1u32 << (numero - 1)) != 0
}

/// Bitmask des directions cardinales du chapeau (1=haut, 2=droite, 4=bas, 8=gauche).
/// Une diagonale arme les deux cardinaux adjacents ; `0` si le D-pad est relâché.
fn cardinaux(chapeau: u8) -> u8 {
    if chapeau >= CHAPEAU_RELACHE {
        return 0;
    }
    let mut masque = 0u8;
    if matches!(chapeau, 7 | 0 | 1) {
        masque |= 1; // haut
    }
    if matches!(chapeau, 1..=3) {
        masque |= 2; // droite
    }
    if matches!(chapeau, 3..=5) {
        masque |= 4; // bas
    }
    if matches!(chapeau, 5..=7) {
        masque |= 8; // gauche
    }
    masque
}

/// Émet un appui (`appui = true`) ou un relâché (`false`) du scan code matériel `scan`.
/// `etendue` ajoute le préfixe « touche étendue » (flèches du pavé directionnel).
#[cfg(windows)]
#[allow(unsafe_code)]
fn envoyer(scan: u16, etendue: bool, appui: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
        KEYEVENTF_SCANCODE, SendInput,
    };

    let mut flags = KEYEVENTF_SCANCODE;
    if etendue {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !appui {
        flags |= KEYEVENTF_KEYUP;
    }
    let entree = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let taille = i32::try_from(core::mem::size_of::<INPUT>()).expect("taille INPUT tient en i32");
    // SAFETY : `entree` est une `INPUT` complète et valide (variante clavier renseignée),
    // on passe exactement 1 élément et la taille réelle de `INPUT`. `SendInput` copie la
    // structure et n'en conserve aucun pointeur après l'appel.
    unsafe {
        SendInput(1, &raw const entree, taille);
    }
}

/// Hors Windows : aucune injection clavier (l'application cible Windows pour le pont).
#[cfg(not(windows))]
fn envoyer(_scan: u16, _etendue: bool, _appui: bool) {}
