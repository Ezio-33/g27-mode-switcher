//! Structure de position d'un device vJoy (miroir de `JOYSTICK_POSITION_V2`).

/// État complet d'un device vJoy, transmis à `UpdateVJD`.
///
/// Miroir **exact** (ordre des champs, types, alignement) de la structure
/// `JOYSTICK_POSITION_V2` du SDK vJoy (`public.h`) : `BYTE` → `u8`, `LONG` →
/// `i32`, `DWORD` → `u32`. Toute divergence de disposition corromprait les
/// données interprétées par le pilote. Le test de taille verrouille la
/// disposition à 108 octets (V2).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct JoystickPositionV2 {
    /// Index 1-basé du device vJoi ciblé.
    pub device: u8,
    /// Axe « throttle ».
    pub throttle: i32,
    /// Axe « rudder ».
    pub rudder: i32,
    /// Axe « aileron ».
    pub aileron: i32,
    /// Axe X.
    pub axis_x: i32,
    /// Axe Y.
    pub axis_y: i32,
    /// Axe Z.
    pub axis_z: i32,
    /// Rotation autour de X.
    pub axis_x_rot: i32,
    /// Rotation autour de Y.
    pub axis_y_rot: i32,
    /// Rotation autour de Z.
    pub axis_z_rot: i32,
    /// Curseur (slider).
    pub slider: i32,
    /// Molette (dial).
    pub dial: i32,
    /// Roue (wheel).
    pub wheel: i32,
    /// Axe VX.
    pub axis_vx: i32,
    /// Axe VY.
    pub axis_vy: i32,
    /// Axe VZ.
    pub axis_vz: i32,
    /// Axe VBRX.
    pub axis_vbrx: i32,
    /// Axe VBRY.
    pub axis_vbry: i32,
    /// Axe VBRZ.
    pub axis_vbrz: i32,
    /// Boutons 1 à 32 (bit `n-1` = bouton `n`).
    pub buttons: i32,
    /// Chapeau POV n°1.
    pub hats: u32,
    /// Chapeau POV n°2.
    pub hats_ex1: u32,
    /// Chapeau POV n°3.
    pub hats_ex2: u32,
    /// Chapeau POV n°4.
    pub hats_ex3: u32,
    /// Boutons 33 à 64.
    pub buttons_ex1: i32,
    /// Boutons 65 à 96.
    pub buttons_ex2: i32,
    /// Boutons 97 à 128.
    pub buttons_ex3: i32,
}

#[cfg(test)]
mod tests {
    use super::JoystickPositionV2;

    #[test]
    fn taille_conforme_au_sdk() {
        // 1 (device) + 3 (padding) + 18 axes × 4 + 4 (boutons) + 4 hats × 4
        // + 3 boutonsEx × 4 = 108 octets.
        assert_eq!(core::mem::size_of::<JoystickPositionV2>(), 108);
        assert_eq!(core::mem::align_of::<JoystickPositionV2>(), 4);
    }
}
