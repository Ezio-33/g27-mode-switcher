//! Mode Forza : retour de force **synthétisé** depuis la télémétrie « Data Out ».
//!
//! Sans masquer le G27 (le jeu le reconnaît nativement → navigation menus/map intacte),
//! l'application écoute le flux **UDP « Data Out »** de Forza Horizon (réglage intégré au
//! jeu, aucun logiciel en plus) et **calcule** le retour de force à partir de la physique
//! (angle de dérive des pneus, vitesse, trottoirs), qu'elle écrit au volant par commandes
//! `lg4ff` brutes. C'est une alternative au pont vJoy : pas de vJoy, pas de HidHide, pas
//! de pilote.
//!
//! Ce sous-module isole : le **parsing** du paquet (ici, pur et testable), le **modèle de
//! force** ([`modele`]) et l'**orchestration** (`pont` : socket UDP + écriture G27).
//!
// « Data Out » est un nom de fonctionnalité Forza, pas un identifiant de code.
#![allow(clippy::doc_markdown)]

mod modele;
mod pont;

pub use modele::{
    ReglagesForza, autocentre_depuis_vitesse, couple_depuis_telemetrie, secousse_depuis_bosse,
};
pub use pont::{ErreurTelemetrie, PontTelemetrie, StatutTelemetrie};

/// Taille de la partie « Sled » du paquet Data Out (octets). Cette portion est
/// **identique entre les titres Forza** (FH5, FH6, Motorsport) : les champs qu'on lit en
/// sont donc stables, indépendamment des ajouts spécifiques à Horizon qui la suivent.
const TAILLE_SLED: usize = 232;

/// Offsets (octets, little-endian) des champs lus dans la partie Sled.
/// Réf. : format « Data Out » documenté (champs ordonnés, struct compacte sans padding).
const OFF_COURSE_ACTIVE: usize = 0; // IsRaceOn : u32 (0 = hors gameplay)
const OFF_ACCEL_Y: usize = 24; // AccelerationY : f32 (vertical : bosses/sauts/atterrissages)
const OFF_VITESSE_X: usize = 32; // VelocityX : f32 (Y, Z suivent à +4, +8)
const OFF_SUSPENSION_AVANT_G: usize = 68; // NormalizedSuspensionTravelFrontLeft : f32 (FR à +4)
const OFF_SUSPENSION_ARRIERE_G: usize = 76; // NormalizedSuspensionTravelRearLeft : f32 (RR à +4)
const OFF_RUMBLE_AVANT_G: usize = 148; // SurfaceRumbleFrontLeft : f32 (FR à +4)
const OFF_DERIVE_AVANT_G: usize = 164; // TireSlipAngleFrontLeft : f32 (FR à +4)

/// Télémétrie décodée utile au retour de force (un sous-ensemble de « Data Out »).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Telemetrie {
    /// `true` si le jeu est en gameplay actif (IsRaceOn). Hors gameplay → aucune force.
    pub course_active: bool,
    /// Vitesse du véhicule (m/s), norme du vecteur vélocité.
    pub vitesse_m_s: f32,
    /// Angle de dérive moyen des pneus **avant** (radians, signé) : source principale du
    /// couple d'auto-alignement (la force dominante ressentie sur un volant).
    pub derive_avant: f32,
    /// Rugosité de surface moyenne **avant** (0..1 ≈ trottoirs/bandes) : effet de texture.
    pub rumble_avant: f32,
    /// Accélération **verticale** (m/s²) : bosses, trottoirs, et surtout sauts/atterrissages
    /// (pic d'accélération à l'impact) → secousses du volant.
    pub accel_vertical: f32,
    /// Débattement de suspension avant **normalisé** (moyenne, `0` = détente max/roues en
    /// l'air, `1` = compression max = chargé) : sert à **alléger** le volant quand le train
    /// avant décolle (sauts) et, avec [`suspension_arriere`](Telemetrie::suspension_arriere),
    /// à déduire le **transfert de charge** avant/arrière.
    pub suspension_avant: f32,
    /// Débattement de suspension arrière **normalisé** (moyenne) : comparé à l'avant, il
    /// donne la **répartition de charge** (freinage → avant chargé → direction plus lourde ;
    /// accélération → avant délesté → plus légère).
    pub suspension_arriere: f32,
}

/// Décode un paquet « Data Out » en [`Telemetrie`]. Renvoie `None` si le paquet est trop
/// court pour contenir la partie Sled (paquet tronqué ou format inattendu).
#[must_use]
pub fn analyser(paquet: &[u8]) -> Option<Telemetrie> {
    if paquet.len() < TAILLE_SLED {
        return None;
    }
    let vx = lire_f32(paquet, OFF_VITESSE_X)?;
    let vy = lire_f32(paquet, OFF_VITESSE_X + 4)?;
    let vz = lire_f32(paquet, OFF_VITESSE_X + 8)?;
    let derive_g = lire_f32(paquet, OFF_DERIVE_AVANT_G)?;
    let derive_d = lire_f32(paquet, OFF_DERIVE_AVANT_G + 4)?;
    let rumble_g = lire_f32(paquet, OFF_RUMBLE_AVANT_G)?;
    let rumble_d = lire_f32(paquet, OFF_RUMBLE_AVANT_G + 4)?;
    let susp_g = lire_f32(paquet, OFF_SUSPENSION_AVANT_G)?;
    let susp_d = lire_f32(paquet, OFF_SUSPENSION_AVANT_G + 4)?;
    let susp_arr_g = lire_f32(paquet, OFF_SUSPENSION_ARRIERE_G)?;
    let susp_arr_d = lire_f32(paquet, OFF_SUSPENSION_ARRIERE_G + 4)?;
    Some(Telemetrie {
        course_active: lire_u32(paquet, OFF_COURSE_ACTIVE)? != 0,
        vitesse_m_s: (vx * vx + vy * vy + vz * vz).sqrt(),
        derive_avant: f32::midpoint(derive_g, derive_d),
        rumble_avant: f32::midpoint(rumble_g, rumble_d),
        accel_vertical: lire_f32(paquet, OFF_ACCEL_Y)?,
        suspension_avant: f32::midpoint(susp_g, susp_d),
        suspension_arriere: f32::midpoint(susp_arr_g, susp_arr_d),
    })
}

/// Lit un `u32` little-endian à l'offset `off` (borné).
fn lire_u32(buf: &[u8], off: usize) -> Option<u32> {
    let octets = buf.get(off..off + 4)?;
    Some(u32::from_le_bytes(octets.try_into().ok()?))
}

/// Lit un `f32` little-endian à l'offset `off` (borné).
fn lire_f32(buf: &[u8], off: usize) -> Option<f32> {
    let octets = buf.get(off..off + 4)?;
    Some(f32::from_le_bytes(octets.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::{OFF_COURSE_ACTIVE, OFF_DERIVE_AVANT_G, OFF_VITESSE_X, TAILLE_SLED, analyser};

    /// Construit un paquet Sled minimal avec les champs voulus.
    fn paquet(course: u32, vx: f32, derive_g: f32, derive_d: f32) -> Vec<u8> {
        let mut p = vec![0u8; TAILLE_SLED];
        p[OFF_COURSE_ACTIVE..OFF_COURSE_ACTIVE + 4].copy_from_slice(&course.to_le_bytes());
        p[OFF_VITESSE_X..OFF_VITESSE_X + 4].copy_from_slice(&vx.to_le_bytes());
        p[OFF_DERIVE_AVANT_G..OFF_DERIVE_AVANT_G + 4].copy_from_slice(&derive_g.to_le_bytes());
        p[OFF_DERIVE_AVANT_G + 4..OFF_DERIVE_AVANT_G + 8].copy_from_slice(&derive_d.to_le_bytes());
        p
    }

    #[test]
    fn paquet_trop_court_rejete() {
        assert!(analyser(&[0u8; 100]).is_none());
    }

    #[test]
    fn decode_les_champs_cles() {
        let t = analyser(&paquet(1, 30.0, 0.1, 0.3)).expect("paquet valide");
        assert!(t.course_active);
        assert!(
            (t.vitesse_m_s - 30.0).abs() < 0.01,
            "vitesse={}",
            t.vitesse_m_s
        );
        // Dérive avant = moyenne gauche/droite.
        assert!(
            (t.derive_avant - 0.2).abs() < 1e-6,
            "derive={}",
            t.derive_avant
        );
    }

    #[test]
    fn course_inactive_lue() {
        let t = analyser(&paquet(0, 0.0, 0.0, 0.0)).expect("paquet valide");
        assert!(!t.course_active);
    }
}
