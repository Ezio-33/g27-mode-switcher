# G27 Mode Switcher

[![CI](https://github.com/Ezio-33/g27-mode-switcher/actions/workflows/ci.yml/badge.svg)](https://github.com/Ezio-33/g27-mode-switcher/actions/workflows/ci.yml)

Bascule un volant **Logitech G27** de son mode dégradé par défaut vers son
**mode natif** (900° de rotation, pédales séparées, axes complets),
**sans installer Logitech Gaming Software (LGS) ni le moindre pilote noyau
propriétaire** — donc **compatible avec HVCI / Memory Integrity** activé sur
Windows 11.

> **État : `v1.2.0` — application adaptative « LGS-like » complète, toujours sans pilote.**
> Au-delà de la bascule en mode natif (axes, angle, autocentrage), l'application offre
> **deux modes de jeu** sélectionnables dans un menu **« Jeux »** :
>
> - **Mode général** — c'est le mode « **passe-partout** » : l'outil crée une **manette
>   virtuelle** (via vJoy), y recopie en continu tout ce que fait votre volant (axes,
>   boutons, pédales), **cache le vrai G27 au jeu** (via HidHide) pour éviter qu'il
>   apparaisse en double, puis **récupère le retour de force que le jeu envoie à la manette
>   virtuelle et le rejoue sur le G27** (force constante + **autocentrage modulé par la
>   vitesse** + **vibrations** du jeu — trottoirs, collisions, rumble). Avantage : ça marche
>   avec **n'importe quel jeu**. Coût : il faut installer **vJoy** + **HidHide** (deux petits
>   pilotes signés, une fois).
> - **Mode Forza** *(nouveau)* — **sans vJoy, sans HidHide, sans rien cacher** : le G27 reste
>   reconnu nativement par le jeu (**navigation menus *et* carte intactes**) et le retour de
>   force est **calculé à partir de la télémétrie « Data Out »** que Forza diffuse lui-même
>   — **aucun logiciel en plus**. Volant **lourd à l'arrêt** qui **s'allège** avec la vitesse,
>   force de virage déduite de la dérive des pneus **et du transfert de charge** (plus lourd
>   au freinage / en appui, plus léger à l'accélération — on « sent » la répartition du poids),
>   **vibrations** de la route et **secousses** aux sauts/atterrissages.
>
> S'y ajoutent une **interface graphique** (accessible : police lisible, contenu défilable),
> le **mapping complet des boutons** (façade, **boîte en H** + marche arrière) avec une
> **fenêtre de remappage interactive** (clique une case, appuie sur le bouton du volant), et
> la **navigation clavier/souris** depuis le **D-pad** (la *croix directionnelle* du volant)
> pour les jeux qui ignorent les manettes virtuelles. Tout cela **sans désactiver HVCI**.
>
> Tout passe par l'**API HID native** du système (aucun pilote à installer, **plus de
> Zadig**). Le **mode général** nécessite **vJoy** + **HidHide** (détectés au lancement) ;
> le **mode Forza** ne nécessite **rien d'autre que le jeu**. Si vous veniez de la `v0.1.0`
> (qui utilisait WinUSB/Zadig), voir [Migration depuis la v0.1.0](#migration-depuis-la-v010).

## Sommaire

- [Objectif](#objectif)
- [Contexte](#contexte)
- [Comment ça marche (techniquement)](#comment-ça-marche-techniquement)
- [Prérequis](#prérequis)
- [Démarrage rapide](#démarrage-rapide)
- [Installation de l'outil](#installation-de-loutil)
- [Migration depuis la v0.1.0](#migration-depuis-la-v010)
- [Utilisation](#utilisation)
- [Configuration](#configuration)
- [Réglage de l'angle de rotation](#réglage-de-langle-de-rotation)
- [Autocentrage](#autocentrage)
- [Retour de force (FFB)](#retour-de-force-ffb)
- [Pont vJoy (recopie d'entrée + masquage)](#pont-vjoy-recopie-dentrée--masquage)
- [Mode Forza (télémétrie, sans aucun logiciel en plus)](#mode-forza-télémétrie-sans-aucun-logiciel-en-plus)
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
- **Rien d'autre** pour la bascule de mode, l'angle, l'autocentrage et le **mode Forza**.

**Dépendances selon le mode de jeu** (menu « Jeux ») :

| Mode | Ce qu'il faut en plus | Pour quels jeux |
| --- | --- | --- |
| **Forza** | **Rien** — la télémétrie « Data Out » est intégrée au jeu | Forza Horizon |
| **Général** | **vJoy** + **HidHide** (pilotes signés WHQL, HVCI-safe, installés une fois) | Tous les jeux |

Le **mode Forza** ne demande donc **aucune installation** : il n'affiche jamais d'alerte
vJoy/HidHide. Ces deux composants ne sont requis (et détectés au lancement) **que** pour
le mode général. Aucun **droit administrateur** n'est nécessaire pour l'outil lui-même.

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

### Interface graphique

**Lancé sans sous-commande**, l'outil ouvre son **interface graphique** (depuis
la v0.3.0) :

```bash
# Double-clic sur l'.exe, ou simplement :
g27-mode-switcher
```

La fenêtre affiche le mode courant du volant en temps réel et regroupe les
actions (bascule en mode natif, angle de rotation avec préréglages, autocentrage,
journal). L'application est **hybride** : lancée depuis un terminal **avec** une
sous-commande, elle se comporte en outil en ligne de commande classique (la
sortie s'affiche dans le terminal) ; sans sous-commande, elle bascule en GUI.

> La GUI est encore en cours de finition visuelle (voir la *dette design* dans
> les notes de développement) ; ses fonctions sont pleinement opérationnelles.

### Ligne de commande

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

## Configuration

Depuis la **v0.3.0**, l'outil mémorise vos réglages dans un fichier **TOML**.
La GUI les enregistre automatiquement (angle, autocentrage, taille et position de
la fenêtre) ; la commande `switch` lit l'angle configuré.

### Emplacement du fichier

| OS | Chemin |
|----|--------|
| **Windows** | `%APPDATA%\g27-mode-switcher\config.toml` |
| **Linux / macOS** | `$XDG_CONFIG_HOME/g27-mode-switcher/config.toml`, sinon `~/.config/g27-mode-switcher/config.toml` |

La commande `g27-mode-switcher config` affiche le chemin exact **et** le contenu
courant. Le dossier est créé automatiquement à la première écriture. Aucun accès
disque n'a lieu en dehors de ce dossier.

### Exemple

```toml
[volant]
angle_par_defaut = 900            # angle appliqué au switch (40–900)
appliquer_angle_au_switch = true  # régler l'angle automatiquement au switch
desactiver_autocentrage_au_switch = false

[fenetre]
largeur = 480.0
hauteur = 800.0
pos_x = 200.0                     # absent au premier lancement
pos_y = 120.0

[journalisation]
verbosite = "info"                # info | debug | trace

[pont]
id_vjoy = 1                       # device vJoy alimenté par le pont (1–16)
masquer_g27_au_demarrage = true   # masquer le G27 réel au jeu quand le pont tourne
```

### Clés

| Clé | Type | Défaut | Rôle |
|-----|------|--------|------|
| `angle_par_defaut` | entier 40–900 | `900` | Angle appliqué par `switch`. |
| `appliquer_angle_au_switch` | booléen | `true` | Régler l'angle lors du `switch`. |
| `desactiver_autocentrage_au_switch` | booléen | `false` | Couper l'autocentrage au `switch`. |
| `verbosite` | `info`/`debug`/`trace` | `info` | Niveau de logs par défaut. |
| `largeur` / `hauteur` / `pos_x` / `pos_y` | nombres | 480×800 | Géométrie de la fenêtre (gérée par la GUI). |
| `id_vjoy` | entier 1–16 | `1` | Device vJoy alimenté par le [pont](#pont-vjoy-recopie-dentrée--masquage). |
| `masquer_g27_au_demarrage` | booléen | `true` | Masquer le G27 réel au jeu quand le pont tourne. |
| `mode_jeu` | `general`/`forza` | `general` | Mode actif au démarrage de la GUI (menu « Jeux »). |
| `forza_port` | port UDP 1–65535 | `5300` | Port d'écoute de la télémétrie [Forza](#mode-forza-télémétrie-sans-aucun-logiciel-en-plus). |
| `forza_gain` | entier 0–100 | `60` | Intensité du retour de force en mode Forza (%). |
| `forza_inverser` | booléen | `false` | Inverser le sens du couple en mode Forza. |

### Modifier la configuration en ligne de commande

```bash
# Afficher le chemin et le contenu courant
g27-mode-switcher config

# Lire une clé
g27-mode-switcher config get angle_par_defaut

# Modifier une clé (valeur validée, puis enregistrée)
g27-mode-switcher config set angle_par_defaut 540
g27-mode-switcher config set verbosite debug
g27-mode-switcher config set id_vjoy 2
g27-mode-switcher config set masquer_g27_au_demarrage false
```

Les booléens acceptent `true`/`false` (ou `vrai`/`faux`, `oui`/`non`, `1`/`0`).
Une clé inconnue ou une valeur invalide est **refusée** avec un message clair,
sans modifier le fichier.

### Comportement si le fichier est absent ou corrompu

Le chargement est **tolérant** : fichier absent, illisible ou TOML invalide →
l'application démarre sur les **valeurs par défaut** (avec un avertissement au
journal), sans jamais bloquer. Les valeurs hors bornes sont **corrigées**
silencieusement (angle ramené dans 40–900, verbosité inconnue → `info`).

### Précédence de la verbosité

Du plus prioritaire au moins prioritaire :

```text
RUST_LOG  >  -v / -vv  >  verbosite (config)  >  info (défaut)
```

## Réglage de l'angle de rotation

En mode natif, le G27 accepte un **angle de rotation** réglable de **40° à
900°**. La commande `switch` applique l'angle **configuré** (`angle_par_defaut`,
900° par défaut — voir [Configuration](#configuration)) ; la commande
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

> ✅ **Nouveau en v0.3.0 : un retour de force du jeu, partiel, sans LGS ni HVCI désactivé.**

Le G27 communique son retour de force via un **protocole FFB propriétaire
Logitech** (commandes spécifiques au-dessus du HID). Le FFB dynamique nécessite une
**couche logicielle qui traduit les effets DirectInput du jeu en commandes FFB
Logitech** — c'est exactement ce que faisait **LGS**. La `v0.3.0` fournit cette
couche via le [**pont vJoy**](#pont-vjoy-recopie-dentrée--masquage) : le jeu envoie
ses effets au device virtuel, l'outil les capte et les rejoue sur le G27 réel.

| Fonctionnalité | En HID natif (v0.3.0) |
| --- | --- |
| Volant, pédales, boutons, boîte H + marche arrière | ✅ Oui |
| Angle de rotation (`set-range`) | ✅ Oui |
| Autocentrage matériel (`set-autocenter`) | ✅ Oui |
| **Force constante du jeu** (poids de la route, auto-alignement en virage) | ✅ Oui (via le pont) |
| **Autocentrage modulé par la vitesse** (ferme à l'arrêt, doux en roulant) | ✅ Oui (via le pont) |
| **Vibrations fines** (collisions, trottoirs, hors-piste) | 🚧 En cours de calibration |

Le pont applique la **force constante** du jeu (validée matériel) et pilote
l'**autocentrage matériel** à partir du ressort que le jeu module avec la vitesse
— d'où une résistance ferme à l'arrêt qui s'adoucit en roulant. Les **effets
périodiques** (vibrations de collision/hors-piste) ne sont pas encore restitués
proprement et restent à calibrer.

> 💡 **Alternative — Logitech Gaming Software (LGS)** : restaure un FFB complet,
> **mais** installe des composants noyau **incompatibles avec HVCI** (il faut alors
> désactiver Memory Integrity, ce que ce projet cherche justement à éviter). Le
> pont vJoy de la `v0.3.0` vise un retour de force **HVCI-safe**.

## Pont vJoy (recopie d'entrée + masquage)

Depuis la **v0.3.0**, l'outil fait le **pont** entre le G27 réel et un **device
vJoy virtuel** : il recopie en continu les axes et boutons du volant vers vJoy,
tout en **masquant le G27 réel** au jeu (via HidHide) pour éviter le doublon. Sur
cette base, le pont **capte les effets FFB** que le jeu envoie au device virtuel et
les **rejoue sur le G27** (force constante + autocentrage modulé — voir
[Retour de force](#retour-de-force-ffb)).

### Prérequis

Deux pilotes **signés WHQL** (donc compatibles HVCI / Memory Integrity), à
installer une fois (x64) :

- **vJoy** — périphérique de manette virtuel : <https://github.com/jshafer817/vJoy/releases>
  (ou <https://sourceforge.net/projects/vjoystick/>). Après installation, ouvrez
  **« Configure vJoy »** et créez au moins le **device #1**.
- **HidHide** — masque le G27 réel aux jeux : <https://github.com/nefarius/HidHide/releases>

La GUI **détecte automatiquement** ces deux composants et vous indique lesquels
manquent ; l'état se met à jour tout seul après installation.

> 💡 Pour le **retour de force**, activez **« Enable Effects »** (FFB) sur le device
> dans *Configure vJoy* : c'est par ce canal que le jeu envoie ses effets, captés
> puis rejoués sur le G27. (Sans FFB, le pont se limite à la recopie des entrées.)

### Utilisation

**Via la GUI** (recommandé) — carte **« Pont vJoy »** :

- **Démarrer le pont** : choisissez le device vJoy (1–16), puis lancez. Le G27 est
  masqué au jeu et le device vJoy recopie le volant. Le device vJoy est **acquis une
  seule fois** pour toute la session.
- **Arrêter le pont** : coupe la recopie et **démasque** le G27 (le device vJoy reste
  réservé jusqu'à la fermeture de l'application). **Démarrer** le relance sans
  ré-acquisition.
- **Fermer la fenêtre** (croix) : nettoie tout — **G27 démasqué + device vJoy
  libéré**.

Le pont tourne sur son **propre thread** : l'interface reste fluide, et la bascule
de mode / l'angle / l'autocentrage continuent de fonctionner **pendant** que le pont
tourne (cet exécutable reste autorisé à lire le G27 grâce à la liste blanche
HidHide).

**Via la ligne de commande** :

```bash
# Démarre le pont (feeder vJoy + masquage). Pour arrêter : FERMEZ la console.
g27-mode-switcher feeder

# Cibler un autre device vJoy, ou ne pas masquer le G27
g27-mode-switcher feeder --id 2
g27-mode-switcher feeder --sans-masquage

# Diagnostiquer les prérequis (vJoy + HidHide)
g27-mode-switcher pont statut
```

Le device et le masquage par défaut viennent de la section `[pont]` de la
[configuration](#configuration) (`id_vjoy`, `masquer_g27_au_demarrage`) ; `--id` et
`--sans-masquage` sont prioritaires.

### Sûreté : le G27 est toujours rendu visible à l'arrêt

Le masquage est **lié au cycle de vie du pont** : il est garanti que le G27 est
**démasqué** et le device vJoy **libéré** à l'arrêt (bouton *Arrêter*), à la
fermeture de la fenêtre, ou à la fermeture de la console (mode CLI) — y compris en
cas d'erreur. Seul un **kill brutal** du process (Gestionnaire des tâches, coupure
de courant) peut laisser le G27 masqué. Dans ce cas rare, rouvrez l'outil et
arrêtez/relancez le pont, ou utilisez le **HidHide Configuration Client** pour vider
la liste de masquage.

## Mode Forza (télémétrie, sans aucun logiciel en plus)

> ✅ **Nouveau : un retour de force pour Forza Horizon sans vJoy, sans HidHide,
> sans masquage — donc avec la navigation des menus *et de la map* 100 % native.**

### Le problème que ça résout

Forza Horizon ne lit **pas** les périphériques DirectInput (comme vJoy) pour sa
navigation d'interface : son curseur de menu/map ne répond qu'au **clavier, à la
souris ou à une manette XInput**. Tant que le G27 est **masqué** (mode général, pour
capter le FFB via vJoy), on perd donc la navigation. À l'inverse, **non masqué**, le
G27 est reconnu nativement (menus + map OK) mais le pilote générique de Windows ne
lui fournit aucun FFB.

Le **mode Forza** lève ce dilemme : on **ne masque pas** le G27 (navigation native
conservée) et l'application **synthétise** le retour de force à partir de la
**télémétrie « Data Out »** que Forza diffuse lui-même (fonction intégrée au jeu,
**aucun logiciel à installer**). La force est calculée depuis la physique et écrite au
volant par commandes `lg4ff` brutes. Le modèle combine quatre effets :

- **force de virage** (couple d'auto-alignement ∝ dérive des pneus avant) ;
- **poids** de direction **lourd à l'arrêt** (friction de parking) qui **s'allège** avec
  la vitesse, comme une vraie direction ;
- **vibrations de la route** (déduites de la variation du débattement de suspension :
  silence sur le lisse, tremblement en tout-terrain) ;
- **secousses** aux **sauts/atterrissages** (compression brutale de la suspension), et
  **allègement** du volant quand le train avant décolle.

| | Mode général (vJoy) | Mode Forza (télémétrie) |
| --- | --- | --- |
| Prérequis | vJoy + HidHide | **Aucun** (Data Out est dans le jeu) |
| Navigation menus | Clavier (G27 masqué) | **Native** (G27 visible) |
| Navigation map | Limitée | **Native** |
| Retour de force | Effets FFB du jeu (tous jeux) | Synthétisé depuis la physique (Forza) |

> ℹ️ Le retour de force du mode Forza est **calculé** (couple d'auto-alignement
> déduit de la dérive des pneus), pas le signal FFB exact du jeu : c'est une très
> bonne approximation, réglable via l'**intensité** et l'**inversion du sens**.

### Procédure — associer la télémétrie au volant

1. **Dans Forza Horizon** : `Réglages` > `HUD et Gameplay` > **« Data Out »** →
   **Activé (On)**.
2. **IP de sortie des données** : `127.0.0.1`.
3. **Port de sortie des données** : un port libre (par défaut **5300**) — **le même**
   que celui réglé dans l'outil.
4. **Dans l'outil** : menu **« Jeux » > « Forza Horizon »**, vérifiez le **port
   d'écoute**, puis **« Démarrer le mode Forza »**.
5. **Lancez une course** : le panneau affiche « Réception OK — course active » et la
   dérive en direct ; le volant reçoit alors le retour de force.

Réglez l'**intensité** à votre goût ; si le volant *fuit* au lieu de résister en
virage, cochez **« Inverser le sens du couple »**.

Le mode Forza pilote lui-même l'**autocentrage matériel** : **lourd à l'arrêt**
(friction de parking) puis **s'allégeant en roulant**, comme une vraie direction —
plus la force de virage déduite de la dérive des pneus. Pas besoin de toucher à la
carte *Autocentrage* (le mode Forza la prend en charge le temps de la session).

### Ligne de commande

```bash
# Démarre le mode Forza (G27 non masqué). Pour arrêter : FERMEZ la console.
g27-mode-switcher forza

# Forcer un port d'écoute (sinon : forza_port de la config)
g27-mode-switcher forza --port 5300
```

Le port, le gain et l'inversion par défaut viennent de la section `[forza]` de la
[configuration](#configuration) (`forza_port`, `forza_gain`, `forza_inverser`). Le
mode actif au démarrage de la GUI est mémorisé (`mode_jeu`).

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
11. ✅ `v0.3.0` : outil adaptatif « façon LGS », sans LGS et **HVCI préservé**.
    - ✅ **Interface graphique** (eframe/egui).
    - ✅ **Configuration TOML** persistante.
    - ✅ **Pont vJoy** : recopie d'entrée G27 → device vJoy + **masquage HidHide**
      du volant réel, avec démasquage garanti à l'arrêt.
    - ✅ **Mapping complet des entrées** : axes, chapeau (POV), boutons façade,
      **boîte de vitesses en H** (vitesses + marche arrière) et **remappage** des
      numéros de boutons — décodage calé sur le **descripteur HID réel** du G27.
    - ✅ **Retour de force partiel** : **force constante** du jeu rejouée sur le G27
      + **autocentrage modulé par la vitesse** (ferme à l'arrêt, doux en roulant).
    - 🚧 **Vibrations fines** (collisions/hors-piste) : à calibrer.
    - 🔜 **Keymapper** (boîte H → clavier) pour les jeux sans remap.
    - 🔜 **Démarrage automatique** avec Windows ; réactivation paramétrable de
      l'autocentrage (`set-autocenter on`).

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

Les polices embarquées dans l'interface graphique sont distribuées sous **SIL
Open Font License 1.1** : **Cinzel** (titres) — voir
[`assets/fonts/OFL.txt`](assets/fonts/OFL.txt) — et **Inter** (corps) — voir
[`assets/fonts/OFL-Inter.txt`](assets/fonts/OFL-Inter.txt).

S'inspirer du comportement *documenté* du noyau Linux (GPL-2.0) sans en copier
le code n'entraîne pas de contamination GPL.
