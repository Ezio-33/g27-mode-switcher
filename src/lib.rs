// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Samuel.V (Ezio_33) — https://la-confrerie-des-ombres.vercel.app

//! Cœur réutilisable du G27 Mode Switcher.
//!
//! Cette bibliothèque regroupe toute la logique métier indépendante de
//! l'interface (CLI ou GUI) : détection HID, construction et envoi des commandes
//! Logitech, bascule de mode, réglage de l'angle et de l'autocentrage. Les
//! binaires (CLI/GUI) la consomment via son API publique, ce qui évite toute
//! duplication entre les frontaux.

pub mod autocenter;
pub mod config;
pub mod device;
pub mod entree;
pub mod feeder;
pub mod hid;
pub mod keymapper;
pub mod range;
pub mod report;
pub mod switcher;
pub mod vjoy;
