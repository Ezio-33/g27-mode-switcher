# G27 Mode Switcher

Bascule un volant **Logitech G27** de son mode dégradé par défaut vers son
**mode natif** (900° de rotation, pédales séparées, retour de force complet),
**sans installer Logitech Gaming Software (LGS) ni le moindre pilote noyau
propriétaire** — donc **compatible avec HVCI / Memory Integrity** activé sur
Windows 11.

> **État du projet : en cours de développement.** L'amorçage (étape 1) est
> terminé. La détection USB, la CLI et la bascule effective arrivent dans les
> étapes suivantes (voir [Feuille de route](#feuille-de-route)).

## Objectif

À son branchement, le G27 démarre en mode de compatibilité « Driving Force EX »
(rotation limitée à 200°, retour de force bridé). Le passage en mode natif G27
se faisait historiquement via le logiciel Logitech (LGS), aujourd'hui
**abandonné** et qui installe des composants noyau **incompatibles avec la
sécurité matérielle de Windows 11**.

Ce projet fournit un **petit binaire Windows autonome** (`.exe`) qui effectue la
bascule en parlant directement au volant en **USB brut**, sans rien installer de
plus côté utilisateur (hormis une étape Zadig décrite plus bas).

## Contexte

- **HVCI / Memory Integrity** (Hypervisor-Protected Code Integrity) est une
  protection de Windows 11 qui refuse de charger des pilotes noyau non conformes.
  Les pilotes hérités de Logitech posent problème une fois cette protection
  active, et beaucoup d'utilisateurs doivent choisir entre désactiver HVCI ou
  perdre leur volant en mode complet.
- **Logitech a abandonné** le support logiciel du G27. La communauté maintient
  donc des solutions alternatives.
- Le mode natif est en réalité **déjà géré nativement par Windows** via son
  pilote HID-compliant générique : il suffit de demander au volant de s'annoncer
  sous son vrai identifiant. C'est exactement ce que fait cet outil.

## Comment ça marche (techniquement)

Le volant expose deux identités USB selon son mode :

| Mode | Vendor ID | Product ID | Caractéristiques |
| --- | --- | --- | --- |
| Compatibilité (défaut) | `0x046D` | `0xC294` | 200°, FFB bridé |
| Natif G27 (cible) | `0x046D` | `0xC29B` | 900°, pédales séparées, FFB complet |

La bascule consiste à envoyer un **« magic packet »** via un *control transfer*
USB (requête `SET_REPORT` de la classe HID). Le format de ce paquet est repris,
**à titre de référence documentaire uniquement**, du pilote Linux
`drivers/hid/hid-lg4ff.c` (aucune ligne de code n'est copiée — voir
[Références](#références)) :

```
bmRequestType = 0x21   (OUT | Class | Interface)
bRequest      = 0x09   (SET_REPORT)
wValue        = 0x0203 (output report, report ID 3)
wIndex        = 0x0000
data          = [0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00]
```

Après réception, le volant **simule une reconnexion USB** et réapparaît sous le
PID `0xC29B`. Windows lui applique alors **automatiquement son pilote manette de
jeu HID natif** (signé Microsoft, sans composant Logitech) — donc **sans rien
qui contrarie HVCI**.

## Prérequis

- **Windows 11** (l'outil cible cette plateforme ; HVCI peut rester activé).
- Un **volant Logitech G27** branché en USB.
- Le pilote **WinUSB** posé **uniquement sur l'interface du G27**, via
  l'utilitaire **[Zadig](https://zadig.akeo.ie/)**. WinUSB est un pilote
  Microsoft signé : il permet à un programme user-mode de dialoguer en USB brut
  sans pilote propriétaire. (Procédure détaillée à venir dans la documentation
  utilisateur.)

> ⚠️ Avec Zadig, ne remplacez le pilote **que** pour le périphérique G27.
> Modifier le pilote d'un autre périphérique (clavier, souris, hub…) peut le
> rendre inutilisable.

## Installation

### Option A — binaire pré-compilé (à venir)

Une fois la première version publiée, téléchargez `g27-mode-switcher.exe` depuis
la page **Releases** du dépôt et placez-le où vous voulez. Aucune installation
n'est requise.

### Option B — compilation depuis les sources

Prérequis de build : [Rust](https://rustup.rs/) stable (la version est épinglée
par `rust-toolchain.toml`).

```bash
# Build natif (plateforme courante)
cargo build --release

# Cross-compilation Windows depuis Linux / WSL2
# (nécessite mingw-w64 et la cible : rustup target add x86_64-pc-windows-gnu)
cargo build --release --target x86_64-pc-windows-gnu
# Binaire : target/x86_64-pc-windows-gnu/release/g27-mode-switcher.exe
```

## Utilisation

> Interface CLI **prévue** (non encore implémentée — voir l'état du projet).

```bash
# Lister les périphériques Logitech détectés
g27-mode-switcher list

# Afficher l'état (mode courant) du volant
g27-mode-switcher status

# Basculer le volant en mode natif G27
g27-mode-switcher switch

# Simuler sans rien envoyer au matériel
g27-mode-switcher switch --dry-run

# Logs détaillés
g27-mode-switcher switch --verbose
```

L'outil fonctionne en **user-mode** : aucun droit administrateur n'est requis.

## Limitations

- Outil spécifique au **Logitech G27** (VID `0x046D`, PID `0xC294` → `0xC29B`).
  Les autres volants ne sont pas pris en charge.
- L'étape **Zadig** (pose du pilote WinUSB) reste **manuelle** et préalable.
- La bascule n'est **pas persistante** : le volant revient en mode compat à
  chaque rebranchement / redémarrage. Il faut relancer l'outil.
- Projet **non affilié à Logitech**. Utilisation **à vos risques** ; testé sur
  un parc matériel limité.

## Feuille de route

1. ✅ Amorçage du projet (outillage, licence, configuration).
2. ⏳ Module `usb` : détection des périphériques Logitech, parsing VID/PID.
3. ⏳ Module `switcher` : construction et envoi du magic packet.
4. ⏳ CLI `clap` : sous-commandes `list` / `switch` / `status`.
5. ⏳ Tests unitaires.
6. ⏳ Cross-compilation Windows validée.
7. ⏳ Intégration continue (GitHub Actions).
8. ⏳ Documentation utilisateur (procédure Zadig détaillée, dépannage).
9. ⏳ Première version `v0.1.0`.

## Références

- Noyau Linux — `drivers/hid/hid-lg4ff.c`
  (<https://github.com/torvalds/linux/blob/master/drivers/hid/hid-lg4ff.c>) :
  utilisé **uniquement comme référence documentaire** du format des paquets de
  bascule de mode. Aucun code source n'est copié ; le comportement est
  réimplémenté en Rust.
- Projet `lg4ff_userspace`
  (<https://github.com/Kethen/lg4ff_userspace>) : référence pour l'approche
  user-space.
- [Zadig](https://zadig.akeo.ie/) — installation du pilote WinUSB.

## Licence

Distribué sous licence **MIT**. Voir le fichier [LICENSE](LICENSE).

S'inspirer du comportement *documenté* du noyau Linux (GPL-2.0) sans en copier
le code n'entraîne pas de contamination GPL.
