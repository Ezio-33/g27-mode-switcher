# G27 Mode Switcher

[![CI](https://github.com/Ezio-33/g27-mode-switcher/actions/workflows/ci.yml/badge.svg)](https://github.com/Ezio-33/g27-mode-switcher/actions/workflows/ci.yml)

Bascule un volant **Logitech G27** de son mode dégradé par défaut vers son
**mode natif** (900° de rotation, pédales séparées, retour de force complet),
**sans installer Logitech Gaming Software (LGS) ni le moindre pilote noyau
propriétaire** — donc **compatible avec HVCI / Memory Integrity** activé sur
Windows 11.

> **État : `v0.2.0` — nouvelle architecture HID native.**
> L'outil parle au volant via l'**API HID native** du système (plus aucun
> pilote à installer, **plus de Zadig**). Il suffit de lancer l'`.exe`.
> Si vous veniez de la `v0.1.0` (qui utilisait WinUSB/Zadig), voir
> [Migration depuis la v0.1.0](#migration-depuis-la-v010).

## Sommaire

- [Objectif](#objectif)
- [Contexte](#contexte)
- [Comment ça marche (techniquement)](#comment-ça-marche-techniquement)
- [Prérequis](#prérequis)
- [Démarrage rapide](#démarrage-rapide)
- [Installation de l'outil](#installation-de-loutil)
- [Migration depuis la v0.1.0](#migration-depuis-la-v010)
- [Utilisation](#utilisation)
- [Dépannage](#dépannage)
- [Annexe : accès HID sous Linux (règle udev)](#annexe--accès-hid-sous-linux-règle-udev)
- [Limitations](#limitations)
- [Feuille de route](#feuille-de-route)
- [Références](#références)
- [Licence](#licence)

## Objectif

À son branchement, le G27 démarre en mode de compatibilité « Driving Force EX »
(rotation limitée à 200°, retour de force bridé). Le passage en mode natif G27
se faisait historiquement via le logiciel Logitech (LGS), aujourd'hui
**abandonné** et qui installe des composants noyau **incompatibles avec la
sécurité matérielle de Windows 11**.

Ce projet fournit un **petit binaire Windows autonome** (`.exe`) qui effectue la
bascule en envoyant une **commande HID standard** au volant, **sans rien
installer de plus** côté utilisateur : pas de pilote, pas de Zadig.

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

La bascule consiste à envoyer un **« magic packet »** : un **HID output report**
(report ID 3). Côté USB bas niveau, il correspond à la requête `SET_REPORT` de la
classe HID. Le format est repris, **à titre de référence documentaire
uniquement**, du pilote Linux `drivers/hid/hid-lg4ff.c` (aucune ligne de code
n'est copiée — voir [Références](#références)) :

```
report ID = 0x03
payload   = [0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00]

# Buffer effectivement écrit (report ID en tête) :
# [0x03, 0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00]
```

L'outil ouvre le périphérique via l'**API HID native** (`hidraw` sous Linux,
`HidUsb`/`setupapi` sous Windows) et écrit ce report. **Le pilote HID du système
reste en place** : aucun pilote n'est remplacé, aucun privilège n'est requis.

Après réception, le volant **simule une reconnexion USB** et réapparaît sous le
PID `0xC29B`. Windows lui applique alors **automatiquement son pilote manette de
jeu HID natif** (signé Microsoft, sans composant Logitech) — donc **sans rien
qui contrarie HVCI**.

> 💡 **Pourquoi pas de pilote USB brut (WinUSB) ?** Une approche USB raw type
> WinUSB doit **déposséder** le pilote HID du volant. Or le firmware du G27 en
> mode compat attend un dialogue HID : privé de son pilote HID, il part en
> **boucle d'énumération USB infinie** (le volant tourne et émet des sons de
> branchement/débranchement en continu). En restant sur le pilote HID natif,
> on évite complètement ce piège — d'où l'abandon de WinUSB/Zadig en `v0.2.0`.

## Prérequis

- **Windows 11** (l'outil cible cette plateforme ; HVCI peut rester activé) —
  fonctionne aussi sous Windows 10.
- Un **volant Logitech G27** branché en USB.
- **Rien d'autre** : aucun pilote, aucun utilitaire tiers, aucun droit
  administrateur.

> 🐧 Sous **Linux**, l'outil fonctionne aussi (backend `hidraw`). L'accès au
> périphérique peut nécessiter une petite **règle udev** — voir
> [l'annexe dédiée](#annexe--accès-hid-sous-linux-règle-udev).

## Démarrage rapide

1. Récupérez `g27-mode-switcher.exe` (voir [Installation](#installation-de-loutil)).
2. Branchez le G27.
3. Vérifiez l'état : `g27-mode-switcher status`.
4. Basculez : `g27-mode-switcher switch`. Le volant se reconnecte en mode natif.

C'est tout. Aucune étape d'installation de pilote.

## Installation de l'outil

### Option A — binaire pré-compilé

- **Releases** : téléchargez `g27-mode-switcher.exe` depuis la page **Releases**
  du dépôt et placez-le où vous voulez. Aucune installation n'est requise.
- **Artifacts de CI** : chaque exécution de l'intégration continue publie aussi
  l'`.exe` en *artifact* téléchargeable depuis l'onglet **Actions** du dépôt.

> ⚠️ **Binaire non signé — avertissement Windows SmartScreen.**
> L'exécutable n'étant pas encore signé numériquement, Windows peut afficher au
> premier lancement « Windows a protégé votre PC » avec « Éditeur : Inconnu ».
> C'est normal pour un binaire open-source non signé. Pour l'exécuter :
> cliquez sur **Informations complémentaires → Exécuter quand même**.
> Par prudence, ne téléchargez l'`.exe` que depuis les **Releases officielles**
> de ce dépôt.
>
> *(La signature de code — via le programme open-source de
> [SignPath](https://signpath.io/) — est envisagée pour une version future.)*

### Option B — compilation depuis les sources

Prérequis de build : [Rust](https://rustup.rs/) (la version est épinglée par
`rust-toolchain.toml`, installée automatiquement par `rustup`). Sous Linux, le
backend `hidraw` de hidapi requiert `libudev` (paquet `libudev-dev`).

```bash
# Build natif (plateforme courante)
cargo build --release

# Cross-compilation Windows depuis Linux / WSL2
# (nécessite mingw-w64 et la cible : rustup target add x86_64-pc-windows-gnu)
cargo build --release --target x86_64-pc-windows-gnu
# Binaire : target/x86_64-pc-windows-gnu/release/g27-mode-switcher.exe
```

## Migration depuis la v0.1.0

La `v0.1.0` reposait sur un pilote **WinUSB** posé via **Zadig** sur le G27.
La `v0.2.0` n'en a plus besoin — au contraire, **laisser WinUSB en place
empêche le volant de fonctionner** (boucle d'énumération décrite plus haut).

Si vous aviez installé WinUSB sur votre G27, **désinstallez-le** pour rendre le
pilote HID natif au volant :

1. Ouvrez un terminal en **administrateur** (PowerShell ou invite de commandes).
2. Listez les pilotes tiers installés et repérez celui associé au G27 (fournisseur
   `WinUSB` / `libusbK` / `libusb-win32`, souvent nommé `oemXX.inf`) :

   ```powershell
   pnputil /enum-drivers
   ```

3. Supprimez ce pilote (remplacez `oemXX.inf` par le nom réel relevé à l'étape 2 ;
   dans notre cas de test il s'agissait de `oem96.inf`) :

   ```powershell
   pnputil /delete-driver oemXX.inf /uninstall /force
   ```

4. **Débranchez puis rebranchez** le G27. Windows lui réassocie son pilote HID
   natif (`HidUsb`). Vérifiez avec `g27-mode-switcher status`, puis basculez
   normalement avec `g27-mode-switcher switch`.

> Alternative graphique : dans le **Gestionnaire de périphériques**, faites un
> clic droit sur le G27 → *Désinstaller l'appareil* en cochant *Supprimer le
> pilote*, puis rebranchez le volant.

## Utilisation

L'outil fonctionne en **user-mode** : aucun droit administrateur n'est requis.

```bash
# Aide générale et version
g27-mode-switcher --help
g27-mode-switcher --version

# Afficher le mode courant du G27
g27-mode-switcher status

# Lister tous les périphériques Logitech détectés
g27-mode-switcher list

# Basculer le volant en mode natif
g27-mode-switcher switch

# Simuler la bascule sans rien envoyer au matériel
g27-mode-switcher switch --dry-run

# Logs détaillés (-v : debug, -vv : trace)
g27-mode-switcher -v switch
```

La verbosité est aussi pilotable via la variable d'environnement `RUST_LOG`
(par ex. `RUST_LOG=debug`), prioritaire sur `-v`.

> 🔁 La bascule **n'est pas persistante** : le volant revient en mode compat à
> chaque rebranchement / redémarrage. Relancez simplement `switch`. Vous pouvez
> automatiser ce lancement au démarrage de Windows (raccourci dans le dossier
> *Démarrage*, ou tâche planifiée).

## Dépannage

**« Aucun G27 détecté » alors que le volant est branché.**
- Vérifiez le câble et le port USB.
- Confirmez qu'il s'agit bien d'un **G27** (`g27-mode-switcher list` affiche les
  VID/PID des périphériques Logitech détectés).
- Sous **Linux**, l'accès `hidraw` peut être refusé sans règle udev — voir
  [l'annexe](#annexe--accès-hid-sous-linux-règle-udev).

**Le volant part en boucle (rotation + sons de branchement/débranchement en
continu), ou « Aucun G27 détecté » juste après avoir installé un pilote.**
- C'est le **piège WinUSB** : un pilote USB brut (WinUSB/libusbK, typiquement posé
  via **Zadig**) a été associé au G27 et le prive de son pilote HID. Le firmware
  reboucle son énumération. **Solution : désinstaller ce pilote** pour rendre le
  HID natif au volant — voir [Migration depuis la v0.1.0](#migration-depuis-la-v010)
  (`pnputil /delete-driver oemXX.inf /uninstall /force`).

**La bascule semble réussir mais le volant ne change pas de mode.**
- Relancez avec les logs détaillés : `g27-mode-switcher -vv switch`. Le trace
  affiche la **collection HID ciblée** (`path`, `interface`, `usage_page`,
  `usage`) juste avant l'envoi. Sous Windows, un G27 peut exposer plusieurs
  collections HID ; ces informations aident à diagnostiquer un report envoyé à la
  mauvaise cible. Joignez-les à un rapport de bug.

**« Le G27 est déjà en mode natif : rien à faire. »**
- Le volant est déjà en `046D:C29B`. Aucune action nécessaire.

**HVCI / Memory Integrity refuse toujours quelque chose.**
- Cet outil **n'installe aucun pilote noyau**. Si HVCI bloque un composant,
  c'est qu'un pilote tiers (souvent un reste de **LGS**) est en cause —
  désinstallez-le.

**Windows affiche « Éditeur inconnu » / SmartScreen, ou l'antivirus s'inquiète.**
- Le binaire n'est **pas encore signé** : c'est attendu. Lancez-le via
  **Informations complémentaires → Exécuter quand même**, et ne le téléchargez
  que depuis les **Releases officielles**. La signature de code est envisagée
  pour une version future (voir [Installation](#installation-de-loutil)).

## Annexe : accès HID sous Linux (règle udev)

L'énumération fonctionne sans privilèges, mais **ouvrir** le périphérique en
`hidraw` peut nécessiter des droits. Pour éviter d'avoir à lancer l'outil en
`sudo`, créez une règle udev qui accorde l'accès à votre session :

Créez `/etc/udev/rules.d/99-logitech-g27.rules` avec :

```udev
# Logitech G27 — mode compatibilité (0xC294) et mode natif (0xC29B).
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c294", MODE="0660", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c29b", MODE="0660", TAG+="uaccess"
```

Puis rechargez les règles et rebranchez le volant :

```bash
sudo udevadm control --reload-rules && sudo udevadm trigger
```

`TAG+="uaccess"` délègue l'accès à l'utilisateur de la session locale active
(via systemd-logind), sans avoir à gérer un groupe dédié.

## Limitations

- Outil spécifique au **Logitech G27** (VID `0x046D`, PID `0xC294` → `0xC29B`).
  Les autres volants ne sont pas pris en charge.
- La bascule n'est **pas persistante** : le volant revient en mode compat à
  chaque rebranchement / redémarrage. Il faut relancer l'outil.
- Projet **non affilié à Logitech**. Utilisation **à vos risques** ; testé sur
  un parc matériel limité.

## Feuille de route

1. ✅ Amorçage du projet (outillage, licence, configuration).
2. ✅ Module de détection des périphériques Logitech, parsing VID/PID.
3. ✅ Module `switcher` : construction et envoi du magic packet.
4. ✅ CLI `clap` : sous-commandes `list` / `switch` / `status`.
5. ✅ Tests unitaires (+ tests matériels opt-in via la feature `hardware-tests`).
6. ✅ Cross-compilation Windows (`.exe` autonome).
7. ✅ Intégration continue (GitHub Actions).
8. ✅ Documentation utilisateur.
9. ✅ Première version `v0.1.0`.
10. ✅ `v0.2.0` : passage à l'API HID native (`hidapi`), suppression de Zadig.

## Références

- Noyau Linux — `drivers/hid/hid-lg4ff.c`
  (<https://github.com/torvalds/linux/blob/master/drivers/hid/hid-lg4ff.c>) :
  utilisé **uniquement comme référence documentaire** du format des paquets de
  bascule de mode. Aucun code source n'est copié ; le comportement est
  réimplémenté en Rust.
- Projet `lg4ff_userspace`
  (<https://github.com/Kethen/lg4ff_userspace>) : référence pour l'approche
  user-space.

## Licence

Distribué sous licence **MIT**. Voir le fichier [LICENSE](LICENSE).

S'inspirer du comportement *documenté* du noyau Linux (GPL-2.0) sans en copier
le code n'entraîne pas de contamination GPL.
