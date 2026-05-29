# Projet : G27 Mode Switcher

## Objectif

Basculer le volant **Logitech G27** de son mode dégradé par défaut
(« Driving Force EX », PID `0xC294`, 200° de rotation, FFB limité) vers son
**mode natif G27** (PID `0xC29B`, 900°, pédales séparées, FFB complet) **sans
installer Logitech Gaming Software (LGS) ni aucun pilote kernel propriétaire**,
pour rester compatible avec **HVCI / Memory Integrity** activé sur Windows 11.

La cible finale est un **binaire Windows autonome** (`.exe`) compilé depuis Linux
(WSL2 Ubuntu 24.04) qui parle au G27 via l'**API HID native** (sans pilote tiers,
sans Zadig), sans dépendance externe à installer côté utilisateur.

## Contexte hardware

- **Vendor ID** : `0x046D` (Logitech)
- **Product ID au démarrage** : `0xC294` (Driving Force EX, mode compat)
- **Product ID cible après bascule** : `0xC29B` (G27 mode natif)
- **Magic packet** : la bascule est une **séquence de deux HID output reports
  non numérotés** de 7 octets, repris du kernel Linux `hid-lg4ff.c`
  (`lg4ff_mode_switch_ext09_g27`, `cmd_count = 2`) :
  - Commande 1 — « revert mode upon USB reset » :
    `[0xf8, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00]`
  - Commande 2 — « switch to G27 with detach » :
    `[0xf8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00]`
  - ⚠️ **Ne pas confondre avec le G29** (`lg4ff_mode_switch_ext09_g29`), dont la
    2ᵉ commande est `[0xf8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00]`. Envoyer ce
    paquet à un G27 ne bascule rien (firmware silencieux) — c'était le bug d'origine.
  - **Pas de report ID** : pour hidapi, chaque buffer est préfixé de `0x00`
    (octet « pas de report ID », retiré et non transmis), p. ex.
    `[0x00, 0xf8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00]`.
  - Au niveau USB, le kernel les émet comme des `SET_REPORT` (classe HID :
    `bmRequestType = 0x21`, `bRequest = 0x09`).
- Après l'envoi, le volant simule un reconnect USB et réapparaît avec le PID
  cible. Windows applique alors automatiquement son driver HID-compliant game
  controller natif (sans driver Logitech), HVCI-safe.
- **Important (leçon matérielle, v0.2.0)** : on reste sur le **pilote HID natif**.
  Déposséder ce pilote au profit de WinUSB (approche USB raw type Zadig) place le
  firmware du G27 en mode compat dans une **boucle d'énumération USB infinie** —
  le volant devient inutilisable. D'où l'usage de `hidapi`, pas de `rusb`/WinUSB.

## Stack technique

- **Langage** : Rust stable (pin via `rust-toolchain.toml`)
- **Crate HID** : `hidapi` (binding HID multiplateforme : `hidraw` sous Linux,
  `HidUsb`/`setupapi` sous Windows). Le backend Linux `hidraw` requiert `libudev`
  (paquet `libudev-dev`).
- **Cross-compile** : target `x86_64-pc-windows-gnu` via `mingw-w64`
- **OS de dev** : Ubuntu 24.04 sur WSL2
- **OS cible** : Windows 11 (HVCI activé)
- **Driver côté Windows** : **pilote HID natif** (`HidUsb`, signé Microsoft) —
  aucune installation tierce, plus de Zadig. Sous Windows, `hidapi` s'appuie sur
  les DLL système (`hid.dll`/`setupapi.dll`) → `.exe` autonome.

## Conventions de code

- **Identifiants, types, noms de fonctions, messages d'erreur d'API** : anglais
  (convention Rust)
- **Commentaires explicatifs et messages CLI destinés à l'utilisateur final** :
  français (le projet cible la communauté francophone en priorité)
- **Style** : `rustfmt` par défaut, configuration éventuellement custom via
  `rustfmt.toml`
- **Lint** : `cargo clippy --all-targets --all-features -- -D warnings` doit
  passer sans warning avant tout commit
- **Fichiers courts** : un module = une responsabilité claire. Si un fichier
  dépasse ~200 lignes, le splitter en sous-modules. Pas de doublons.
- **Pas de magic numbers** dans le code : toutes les constantes USB (VID, PID,
  bytes du magic packet, etc.) doivent être déclarées comme `const` nommées en
  haut du module concerné, avec un commentaire indiquant la source (référence
  au kernel Linux).

## Exigences de sécurité (non négociables)

- **Aucun bloc `unsafe`** sauf si strictement nécessaire pour un appel FFI
  bas niveau (ex. `hidapi`), et chaque bloc `unsafe` doit être documenté avec un
  commentaire `// SAFETY:` expliquant pourquoi il est sûr.
- **Aucune dépendance** qui pull du code propriétaire, des binaires
  pré-compilés non sourcés, ou des `build.rs` qui exécutent du code arbitraire
  inconnu. Toute nouvelle dépendance doit être justifiée par une PR/issue.
- **Validation stricte des paramètres** USB avant chaque transfer. Pas de
  paramètres « hardcodés » sans vérification.
- **Pas d'élévation de privilèges** : le binaire doit fonctionner en user-mode
  standard, sans demander de droits admin.
- **Pas d'accès réseau, pas d'accès filesystem** en dehors du dossier du
  binaire et d'un éventuel `~/.config/g27-mode-switcher/` pour la config.
- **Logging structuré** via `tracing` ou `log`+`env_logger`, jamais de
  `println!` en code de production (sauf CLI explicite destinée à
  l'utilisateur).

## Qualité

- **Tests unitaires** pour toute logique métier (parsing, construction du
  magic packet, gestion d'erreurs). Pas obligatoires pour les appels USB
  directs (hardware-dependent).
- **Tests d'intégration** avec un G27 réel : à documenter manuellement dans
  `tests/README.md`, lancement opt-in via feature flag `hardware-tests`.
- **CI GitHub Actions** : build Linux + build cross-compile Windows + clippy +
  fmt check + tests sur chaque push et PR.

## Workflow Git

- **Branche principale** : `main`
- **Branches de feature** : `feat/nom-court`, `fix/nom-court`, `chore/...`
- **Conventional Commits** obligatoires :
  - `feat:` pour une nouvelle fonctionnalité
  - `fix:` pour un bug
  - `docs:` pour la doc
  - `refactor:` pour une réorganisation sans changement fonctionnel
  - `test:` pour des tests
  - `chore:` pour le tooling
  - `ci:` pour le CI
- **Signed commits** souhaités si la clé GPG est configurée
- **Pas de force-push sur `main`**

## Commandes utiles

```bash
# Build debug (Linux)
cargo build

# Build release (Linux)
cargo build --release

# Cross-compile Windows
cargo build --release --target x86_64-pc-windows-gnu
# Le binaire final est à : target/x86_64-pc-windows-gnu/release/g27-mode-switcher.exe

# Tests
cargo test

# Lint strict
cargo clippy --all-targets --all-features -- -D warnings

# Formatage
cargo fmt

# Lister les périphériques USB connectés (utile pour debug)
cargo run -- --list-devices
```

## Références externes (à respecter au niveau licence)

- **Kernel Linux** `drivers/hid/hid-lg4ff.c`
  (https://github.com/torvalds/linux/blob/master/drivers/hid/hid-lg4ff.c)
  — utilisé uniquement comme **référence documentaire** pour le format des
  magic packets de bascule de mode. Aucun code source n'est copié. Le projet
  réimplémente le comportement en Rust.
- **Projet `lg4ff_userspace`** (https://github.com/Kethen/lg4ff_userspace) :
  référence pour l'approche userspace.

Le projet est sous **licence MIT**. Le fait de s'inspirer du comportement
documenté du kernel Linux GPL-2.0 (sans copier de code) n'impose pas de
contamination GPL.

## Plan de développement (à jour au démarrage)

1. **Bootstrap** : `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`,
   `rustfmt.toml`, `clippy.toml`, `.gitignore` adapté Rust, `LICENSE`,
   `README.md`.
2. **Module `hid`** : détection des périphériques Logitech connectés, parsing
   VID/PID, affichage CLI propre.
3. **Module `switcher`** : construction du magic packet, envoi via HID output
   report, gestion des erreurs.
4. **CLI** via `clap` : sous-commandes `list`, `switch`, `status`,
   `--verbose`, `--dry-run`.
5. **Tests** unitaires sur la construction du packet et le parsing.
6. **Cross-compile Windows** : config `.cargo/config.toml` + tests de build.
7. **CI GitHub Actions** : workflow complet.
8. **Documentation utilisateur** : README avec procédure d'installation
   simplifiée, exemples d'usage, troubleshooting.
9. **Tag v0.1.0** et release GitHub.
10. **Refactor v0.2.0** : passage de `rusb`/WinUSB à l'API HID native
    (`hidapi`), suppression de la dépendance à Zadig.

## Notes importantes pour Claude Code

- **Vérifie toujours `Cargo.lock` à jour** avant de commit.
- **Demande confirmation à l'utilisateur** avant tout `git push`, toute
  publication de release, ou toute opération destructive sur le repo.
- **Ne jamais committer** : `target/`, `.env`, ou tout fichier de config
  locale (`.cargo/config.local.toml`).
- **Si une dépendance crate est ajoutée**, vérifier sa popularité (downloads
  crates.io), sa licence (MIT/Apache-2.0 préférés), et sa date de dernière
  maintenance.
- **Sur les bascules USB**, expliquer toujours en français à l'utilisateur ce
  qui va se passer avant d'exécuter, surtout en mode interactif.
