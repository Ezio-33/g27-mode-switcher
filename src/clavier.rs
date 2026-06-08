//! Injection de touches clavier pour la croix directionnelle (D-pad) du G27.
//!
//! Certains jeux (Forza) ne naviguent leurs menus/map qu'avec un périphérique qu'ils
//! **reconnaissent** (vrai volant, manette Xbox) ou avec le **clavier** — jamais avec
//! un device vJoy générique. Quand le G27 est masqué (pour que le retour de force passe
//! par vJoy), on traduit donc le D-pad en **flèches clavier** ↑↓←→ via `SendInput` :
//! Forza les comprend dans les menus, sur la map et en jeu.
//!
//! On n'émet que les **fronts** (appui / relâché) : tenir une direction n'envoie pas de
//! répétition (Windows gère l'auto-répétition d'une touche maintenue). Le `Drop` relâche
//! toute touche encore enfoncée pour ne jamais laisser une flèche « collée ».

/// Valeur du chapeau quand le D-pad est relâché (cf. [`crate::entree::CHAPEAU_RELACHE`]).
const CHAPEAU_RELACHE: u8 = 8;

/// Codes de **scan matériels** des 4 flèches (touches étendues du pavé directionnel).
/// On envoie le scan code plutôt que le code virtuel : c'est ce que lisent la plupart
/// des jeux (entrée bas niveau / `RawInput` / `DirectInput`), pas seulement l'UI Windows.
const SC_UP: u16 = 0x48;
const SC_LEFT: u16 = 0x4B;
const SC_RIGHT: u16 = 0x4D;
const SC_DOWN: u16 = 0x50;

/// Association (bit cardinal, scan code). Bits : 1=haut, 2=droite, 4=bas, 8=gauche.
const TOUCHES: [(u8, u16); 4] = [(1, SC_UP), (2, SC_RIGHT), (4, SC_DOWN), (8, SC_LEFT)];

/// Traduit le chapeau du G27 en frappes de flèches clavier (fronts uniquement).
pub struct ChapeauClavier {
    /// Bitmask des directions cardinales actuellement « enfoncées ».
    cardinaux: u8,
}

impl ChapeauClavier {
    /// Crée l'injecteur (aucune touche enfoncée au départ).
    #[must_use]
    pub fn new() -> Self {
        Self { cardinaux: 0 }
    }

    /// Met à jour les touches d'après la valeur du chapeau (`0–7` = directions,
    /// `0` = haut sens horaire ; `≥8` = relâché). Émet un appui/relâché clavier pour
    /// chaque direction qui change d'état depuis le dernier appel.
    pub fn appliquer(&mut self, chapeau: u8) {
        let actuels = cardinaux(chapeau);
        let changes = actuels ^ self.cardinaux;
        for (bit, touche) in TOUCHES {
            if changes & bit != 0 {
                envoyer(touche, actuels & bit != 0);
            }
        }
        self.cardinaux = actuels;
    }
}

impl Default for ChapeauClavier {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ChapeauClavier {
    fn drop(&mut self) {
        // Relâche toute flèche encore enfoncée (sinon elle resterait « collée »).
        for (bit, touche) in TOUCHES {
            if self.cardinaux & bit != 0 {
                envoyer(touche, false);
            }
        }
    }
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

/// Émet un appui (`appui = true`) ou un relâché (`false`) de la touche de scan code
/// matériel `scan` (touche étendue du pavé directionnel).
#[cfg(windows)]
#[allow(unsafe_code)]
fn envoyer(scan: u16, appui: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
        KEYEVENTF_SCANCODE, SendInput,
    };

    let mut flags = KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY;
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
fn envoyer(_scan: u16, _appui: bool) {}
