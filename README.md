# G27 Mode Switcher

[![CI](https://github.com/Ezio-33/g27-mode-switcher/actions/workflows/ci.yml/badge.svg)](https://github.com/Ezio-33/g27-mode-switcher/actions/workflows/ci.yml)

Bascule un volant **Logitech G27** de son mode dégradé par défaut vers son
**mode natif** (900° de rotation, pédales séparées, axes complets),
**sans installer Logitech Gaming Software (LGS) ni le moindre pilote noyau
propriétaire** — donc **compatible avec HVCI / Memory Integrity** activé sur
Windows 11.

> ℹ️ La v0.2.0 débloque les **axes, l'angle et l'autocentrage** du mode natif,
> mais **pas le retour de force dynamique des jeux** (qui requiert une couche
> pilote) — voir [Retour de force (FFB)](#retour-de-force-ffb). Le FFB complet
> est prévu pour la v0.3.0, sans désactiver HVCI.

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
- [Réglage de l'angle de rotation](#réglage-de-langle-de-rotation)
- [Autocentrage](#autocentrage)
- [Retour de force (FFB)](#retour-de-force-ffb)
- [Mapping natif du G27](#mapping-natif-du-g27)
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
| Natif G27 (cible) | `0x046D` | `0xC29B` | 900°, pédales séparées, FFB matériel débridé |

La bascule consiste à envoyer une **séquence de deux « magic packets »** : deux
**HID output reports non numérotés** de 7 octets. Le format est repris, **à titre
de référence documentaire uniquement**, du pilote Linux
`drivers/hid/hid-lg4ff.c` (`lg4ff_mode_switch_ext09_g27`, aucune ligne de code
n'est copiée — voir [Références](#références)) :

```
# Commande 1 — revert mode upon USB reset
[0xF8, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00]
# Commande 2 — switch to G27 with detach
[0xF8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00]

# Ces commandes n'ont pas de report ID : pour hidapi, le buffer est préfixé de
# 0x00 (octet « pas de report ID », non transmis). Ex. pour la commande 2 :
# [0x00, 0xF8, 0x09, 0x04, 0x01, 0x00, 0x00, 0x00]
```

> ⚠️ À ne pas confondre avec le **G29** : sa 2ᵉ commande est
> `[0xF8, 0x09, 0x05, 0x01, 0x01, 0x00, 0x00]`. Envoyer ce paquet à un G27 ne
> bascule rien (le firmware l'ignore en silence).

L'outil ouvre le périphérique via l'**API HID native** (`hidraw` sous Linux,
`HidUsb`/`setupapi` sous Windows) et écrit ce report. **Le pilote HID du système
reste en place** : aucun pilote n'est remplacé, aucun privilège n'est requis.

Après réception, le volant **simule une reconnexion USB** et réapparaît sous le
PID `0xC29B`. Windows lui applique alors **automatiquement son pilote manette de
jeu HID natif** (signé Microsoft, sans composant Logitech) — donc **sans rien
qui contrarie HVCI**.

Une fois le volant réapparu en mode natif, l'outil règle l'**angle de rotation à
900°** (commande HID dérivée de `lg4ff_set_range_g25`). Il **laisse l'autocentrage
matériel actif** par défaut : sans FFB dynamique (voir
[Retour de force (FFB)](#retour-de-force-ffb)), c'est la seule force de centrage
du volant. Le réglage de l'angle se désactive avec `--no-range` ; l'autocentrage
peut être coupé explicitement avec `--disable-autocenter` (ou
`set-autocenter off`) — utile uniquement si une couche FFB prend le relais.

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
4. Basculez : `g27-mode-switcher switch`. Le volant se reconnecte en mode natif
   et son angle de rotation est réglé sur **900°**. L'**autocentrage matériel
   reste actif** (c'est la seule force de centrage tant qu'il n'y a pas de FFB
   dynamique — voir [Retour de force (FFB)](#retour-de-force-ffb)).

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

# Basculer le volant en mode natif (règle 900°, autocentrage laissé actif)
g27-mode-switcher switch

# Basculer sans régler l'angle, ou en désactivant l'autocentrage matériel
g27-mode-switcher switch --no-range
g27-mode-switcher switch --disable-autocenter

# Simuler la bascule sans rien envoyer au matériel
g27-mode-switcher switch --dry-run

# Régler l'angle de rotation (mode natif requis), de 40° à 900°
g27-mode-switcher set-range 900

# Désactiver l'autocentrage matériel (mode natif requis)
g27-mode-switcher set-autocenter off

# Logs détaillés (-v : debug, -vv : trace)
g27-mode-switcher -v switch
```

La verbosité est aussi pilotable via la variable d'environnement `RUST_LOG`
(par ex. `RUST_LOG=debug`), prioritaire sur `-v`.

> 🔁 La bascule **n'est pas persistante** : le volant revient en mode compat à
> chaque rebranchement / redémarrage. Relancez simplement `switch`. Vous pouvez
> automatiser ce lancement au démarrage de Windows (raccourci dans le dossier
> *Démarrage*, ou tâche planifiée).

## Réglage de l'angle de rotation

En mode natif, le G27 accepte un **angle de rotation** réglable de **40° à
900°**. La commande `switch` applique **900°** par défaut ; la commande
`set-range` permet de choisir une autre valeur, par exemple selon le type de
course :

```bash
g27-mode-switcher set-range 360   # monoplaces / F1
g27-mode-switcher set-range 540   # GT / endurance
g27-mode-switcher set-range 720   # rallye
g27-mode-switcher set-range 900   # camion / simulation (pleine échelle)
```

- Le réglage **exige le mode natif** (`0xC29B`). Si le volant est encore en mode
  compatibilité, l'outil vous invite à lancer `switch` d'abord.
- Une valeur hors de `[40, 900]` est refusée avec un message explicite.
- Pour vérifier l'effet sous Windows : `joy.cpl` → propriétés du volant ; à 900°,
  une rotation complète correspond à **2,5 tours** de volant.

> ℹ️ Beaucoup de jeux imposent leur propre angle de rotation. Le réglage de cet
> outil sert de **valeur par défaut au niveau du firmware**, utile hors jeu ou
> pour les titres qui respectent l'angle matériel.

## Autocentrage

Le G27 embarque un **ressort de rappel au centre géré par son firmware**
(« autocentrage matériel »), réglable indépendamment du FFB des jeux.

> ⚖️ **Pourquoi l'outil le laisse actif par défaut.** L'autocentrage matériel
> n'est pas du vrai retour de force, mais **tant que le FFB dynamique n'est pas
> disponible** (cas de la v0.2.0 — voir [Retour de force (FFB)](#retour-de-force-ffb)),
> il fournit la **seule force de centrage** du volant. Le désactiver rendrait le
> volant complètement **mou**. On ne le coupe donc que si une couche FFB prend le
> relais (LGS, ou la future v0.3.0) — auquel cas le ressort matériel **lutterait**
> contre les effets du jeu, ce que LGS évitait justement en le désactivant.

```bash
# Couper l'autocentrage (seulement si une couche FFB gère déjà le centrage)
g27-mode-switcher set-autocenter off

# … ou directement pendant la bascule
g27-mode-switcher switch --disable-autocenter
```

- Le réglage **exige le mode natif** (`0xC29B`) ; en mode compatibilité, l'outil
  vous invite à lancer `switch` d'abord.
- Il **n'est pas persistant** : l'autocentrage se réinitialise (réactivé) au
  rebranchement du volant.
- La **réactivation paramétrable** (`set-autocenter on` avec force réglable) est
  prévue pour la **v0.3.0** ; en v0.2.0, `on` n'est pas encore implémenté.

> 🔧 La commande dérive de `lg4ff_set_autocenter_default` (cas force nulle) du
> pilote Linux `hid-lg4ff.c` : un report HID `[0xF5, 0x00, …]` (voir
> [Références](#références)).

## Retour de force (FFB)

> ⚠️ **Limitation importante de la v0.2.0 : pas de FFB dynamique des jeux.**

Le G27 communique son retour de force via un **protocole FFB propriétaire
Logitech** (commandes spécifiques au-dessus du HID). En mode HID natif **sans
pilote dédié**, voici ce qui fonctionne et ce qui ne fonctionne pas :

| Fonctionnalité | En HID natif (v0.2.0) |
| --- | --- |
| Volant, pédales, boutons, boîte H | ✅ Oui |
| Angle de rotation (`set-range`) | ✅ Oui |
| Autocentrage matériel (`set-autocenter`) | ✅ Oui |
| **FFB dynamique du jeu** (effets de route, perte d'adhérence, trottoirs…) | ❌ **Non** |

Le FFB dynamique nécessite une **couche logicielle qui traduit les effets
DirectInput du jeu en commandes FFB Logitech** — c'est exactement ce que faisait
le pilote **LGS**. Sans cette couche, le firmware ne reçoit jamais les effets et
le volant reste inerte côté FFB.

L'**autocentrage matériel** (réglable via `set-autocenter`) fournit une **force
de centrage basique** — utile pour ne pas avoir un volant mou — mais **ce n'est
pas du vrai retour de force** : il ignore ce qui se passe dans le jeu.

### En attendant : deux options pour le FFB

1. **Logitech Gaming Software (LGS)** : restaure le FFB complet, **mais** installe
   des composants noyau **incompatibles avec HVCI** — il faut alors **désactiver
   Memory Integrity**, ce que ce projet cherche justement à éviter.
2. **Attendre la v0.3.0** (voir [Feuille de route](#feuille-de-route)).

### Ce que prévoit la v0.3.0

Un **FFB complet en option**, sans LGS et **sans désactiver HVCI**, en s'appuyant
sur **vJoy + HidHide** (pilotes **signés WHQL**, donc compatibles Memory
Integrity) pour exposer un périphérique virtuel et router les effets FFB vers le
G27. Objectif : retrouver un retour de force de jeu tout en restant HVCI-safe.

## Mapping natif du G27

En mode natif, Windows expose le G27 comme une **manette de jeu HID** standard.
Les axes et boutons sont remontés ainsi (utile pour configurer un jeu) :

| Élément | Entrée HID |
| --- | --- |
| Volant | Axe **X** |
| Pédale d'embrayage | Axe **Y** |
| Pédale d'accélérateur | Axe **Z** |
| Pédale de frein | Axe **RotationZ** |
| Boîte H — 1ʳᵉ … 6ᵉ | Boutons **13** à **18** |
| Boîte H — marche arrière | Bouton **23** |

> 🎮 Certains jeux ne permettent pas de **remapper les positions de la boîte H**
> (les six rapports sont vus comme des boutons distincts, pas comme un sélecteur).
> La **v0.3.0** prévue apportera un **mapping boutons → clavier** (plus une
> interface graphique) pour contourner cette limite — voir la
> [feuille de route](#feuille-de-route).

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
10. ✅ `v0.2.0` : passage à l'API HID native (`hidapi`), suppression de Zadig,
    commandes `set-range` et `set-autocenter`, réglage automatique de l'angle à
    900° après bascule (autocentrage matériel laissé actif par défaut).
11. 🔜 `v0.3.0` : **FFB dynamique complet** en option via **vJoy + HidHide**
    (signés WHQL, donc **HVCI préservé**, sans LGS) ; **interface graphique** ;
    **keymapper** (mapping des boutons du G27 — notamment la boîte H — vers des
    touches clavier) pour les jeux qui ne savent pas remapper la boîte H ;
    **réactivation paramétrable** de l'autocentrage (`set-autocenter on`).

## Références

- Noyau Linux — `drivers/hid/hid-lg4ff.c`
  (<https://github.com/torvalds/linux/blob/master/drivers/hid/hid-lg4ff.c>) :
  utilisé **uniquement comme référence documentaire** du format des paquets HID
  (bascule de mode, réglage de l'angle, désactivation de l'autocentrage). Aucun
  code source n'est copié ; le comportement est réimplémenté en Rust.
- Projet `lg4ff_userspace`
  (<https://github.com/Kethen/lg4ff_userspace>) : référence pour l'approche
  user-space.

## Crédits / Auteur

Projet créé et maintenu par **Samuel.V** — *Ezio_33*.

- 🌐 Site : <https://la-confrerie-des-ombres.vercel.app>
- 💬 Discord : <https://discord.gg/zckGmdg>
- ❤️ Soutenir : <https://streamelements.com/ezio_33/tip>

Si cet outil vous est utile, un passage sur le site ou le Discord fait toujours
plaisir. Merci de **conserver l'attribution** (auteur + lien) en cas de
réutilisation — c'est ce que demande la clause de notice de la licence MIT.

## Licence

Distribué sous licence **MIT** — voir [`LICENSE`](LICENSE) (version anglaise
canonique, **seule juridiquement valable**). Une traduction française
**indicative** est disponible dans [`LICENSE.fr.md`](LICENSE.fr.md).

La police **Cinzel** embarquée dans l'interface graphique (titres) est
distribuée sous **SIL Open Font License 1.1** — voir
[`assets/fonts/OFL.txt`](assets/fonts/OFL.txt).

S'inspirer du comportement *documenté* du noyau Linux (GPL-2.0) sans en copier
le code n'entraîne pas de contamination GPL.
