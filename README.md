# G27 Mode Switcher

Bascule un volant **Logitech G27** de son mode dégradé par défaut vers son
**mode natif** (900° de rotation, pédales séparées, retour de force complet),
**sans installer Logitech Gaming Software (LGS) ni le moindre pilote noyau
propriétaire** — donc **compatible avec HVCI / Memory Integrity** activé sur
Windows 11.

> **État : fonctionnel, en préparation de la première version `v0.1.0`.**
> La détection USB, la CLI (`list` / `switch` / `status`) et la bascule sont
> opérationnelles. Testé sur un parc matériel limité — les retours sont
> bienvenus.

## Sommaire

- [Objectif](#objectif)
- [Contexte](#contexte)
- [Comment ça marche (techniquement)](#comment-ça-marche-techniquement)
- [Prérequis](#prérequis)
- [Démarrage rapide](#démarrage-rapide)
- [Installation de l'outil](#installation-de-loutil)
- [Installation du pilote WinUSB (Zadig)](#installation-du-pilote-winusb-zadig)
- [Utilisation](#utilisation)
- [Dépannage](#dépannage)
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
  l'utilitaire **[Zadig](https://zadig.akeo.ie/)** — voir
  [la procédure détaillée](#installation-du-pilote-winusb-zadig). WinUSB est un
  pilote Microsoft signé qui permet à un programme user-mode de dialoguer en USB
  brut, sans pilote propriétaire.

> ⚠️ Avec Zadig, ne remplacez le pilote **que** pour le périphérique G27.
> Modifier le pilote d'un autre périphérique (clavier, souris, hub…) peut le
> rendre inutilisable.

## Démarrage rapide

1. Récupérez `g27-mode-switcher.exe` (voir [Installation](#installation-de-loutil)).
2. Posez le pilote **WinUSB** sur le G27 avec **Zadig** (une seule fois — voir
   [la procédure](#installation-du-pilote-winusb-zadig)).
3. Vérifiez l'état : `g27-mode-switcher status`.
4. Basculez : `g27-mode-switcher switch`. Le volant se reconnecte en mode natif.

## Installation de l'outil

### Option A — binaire pré-compilé

- **Releases** (à partir de la `v0.1.0`) : téléchargez `g27-mode-switcher.exe`
  depuis la page **Releases** du dépôt et placez-le où vous voulez. Aucune
  installation n'est requise.
- **Artifacts de CI** : chaque exécution de l'intégration continue publie aussi
  l'`.exe` en *artifact* téléchargeable depuis l'onglet **Actions** du dépôt.

### Option B — compilation depuis les sources

Prérequis de build : [Rust](https://rustup.rs/) (la version est épinglée par
`rust-toolchain.toml`, installée automatiquement par `rustup`).

```bash
# Build natif (plateforme courante)
cargo build --release

# Cross-compilation Windows depuis Linux / WSL2
# (nécessite mingw-w64 et la cible : rustup target add x86_64-pc-windows-gnu)
cargo build --release --target x86_64-pc-windows-gnu
# Binaire : target/x86_64-pc-windows-gnu/release/g27-mode-switcher.exe
```

## Installation du pilote WinUSB (Zadig)

Cette étape **manuelle et unique** autorise l'outil à parler au G27 en USB brut.
Elle s'effectue une seule fois par machine (Windows mémorise ensuite le pilote
pour ce périphérique).

> ℹ️ **Droits administrateur** : seul **Zadig** en a besoin (il remplace un
> pilote). L'outil `g27-mode-switcher`, lui, fonctionne en **user-mode**, sans
> privilèges élevés.

1. **Branchez le G27.** Au démarrage il est en mode compatibilité, donc visible
   sous l'USB ID `046D:C294`.
2. **Téléchargez Zadig** depuis <https://zadig.akeo.ie/> (exécutable portable,
   aucune installation).
3. **Lancez Zadig** en tant qu'administrateur (clic droit → *Exécuter en tant
   qu'administrateur*).
4. Menu **`Options` → cochez `List All Devices`**.
5. Dans la **liste déroulante**, sélectionnez l'entrée du G27. Pour être sûr de
   la bonne : son **USB ID doit être `046D C294`** (affiché par Zadig). Si
   plusieurs interfaces apparaissent, choisissez **`(Interface 0)`**.
6. À droite de la flèche, vérifiez que le pilote cible est **`WinUSB`**.
7. Cliquez sur **`Replace Driver`** (ou `Install Driver`) et confirmez.
8. Attendez le message de réussite, puis **débranchez/rebranchez** le volant.

> 📷 *Captures à ajouter ici :* (a) `Options → List All Devices` coché ;
> (b) le G27 `046D:C294` sélectionné avec la cible `WinUSB` ; (c) l'écran de
> confirmation après `Replace Driver`.

> 🔁 Après une bascule réussie, le volant repasse sous `046D:C29B`. C'est
> Windows qui gère alors ce nouvel ID avec son pilote HID natif — **rien à
> refaire dans Zadig**. Le pilote WinUSB reste associé au mode compat `046D:C294`
> pour les prochaines bascules.

## Utilisation

La CLI est implémentée (sous-commandes `list` / `switch` / `status`). L'outil
fonctionne en **user-mode** : aucun droit administrateur n'est requis.

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

## Dépannage

**« Aucun G27 détecté » alors que le volant est branché.**
- Vérifiez le câble et le port USB.
- Assurez-vous d'avoir posé **WinUSB** sur le bon périphérique
  ([procédure Zadig](#installation-du-pilote-winusb-zadig)) — sans cela, le
  périphérique n'est pas accessible en USB brut.
- Confirmez qu'il s'agit bien d'un **G27** (`g27-mode-switcher list` affiche les
  VID/PID).

**La bascule échoue avec une erreur d'accès USB.**
- Le pilote WinUSB n'est probablement pas (ou plus) associé à l'interface du
  G27 : relancez Zadig sur l'entrée `046D:C294`, **`(Interface 0)`**.
- Un autre logiciel utilise peut-être le volant (jeu, LGS résiduel) : fermez-le
  puis réessayez.

**Le volant est revenu en mode compatibilité après un redémarrage / rebranchement.**
- C'est attendu : la bascule **n'est pas persistante**. Relancez simplement
  `g27-mode-switcher switch`. Vous pouvez automatiser ce lancement au démarrage
  de Windows (raccourci dans le dossier *Démarrage*, ou tâche planifiée).

**« Le G27 est déjà en mode natif : rien à faire. »**
- Le volant est déjà en `046D:C29B`. Aucune action nécessaire.

**HVCI / Memory Integrity refuse toujours quelque chose.**
- Cet outil **n'installe aucun pilote noyau**. Si HVCI bloque un composant,
  c'est qu'un pilote tiers (souvent un reste de **LGS**) est en cause —
  désinstallez-le. WinUSB, lui, est signé par Microsoft et HVCI-safe.

**Côté développement Linux : erreur de permission USB.**
- L'énumération fonctionne sans privilèges, mais l'ouverture du périphérique
  peut nécessiter une règle `udev` adaptée ou une exécution privilégiée.

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
2. ✅ Module `usb` : détection des périphériques Logitech, parsing VID/PID.
3. ✅ Module `switcher` : construction et envoi du magic packet.
4. ✅ CLI `clap` : sous-commandes `list` / `switch` / `status`.
5. ✅ Tests unitaires (+ tests matériels opt-in via la feature `hardware-tests`).
6. ✅ Cross-compilation Windows validée (`.exe` autonome, libusb statique).
7. ✅ Intégration continue (GitHub Actions).
8. ✅ Documentation utilisateur (procédure Zadig détaillée, dépannage).
9. ⏳ Première version `v0.1.0` et release GitHub.

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
