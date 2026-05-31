# Tests

## Tests unitaires (toujours actifs)

La logique métier pure est couverte par des tests unitaires situés dans les
modules (`src/hid.rs`, `src/report.rs`, `src/switcher.rs`, `src/range.rs`,
`src/autocenter.rs`) : classification VID/PID, construction et validation des
reports HID (bascule, réglage d'angle, désactivation de l'autocentrage), rendu
d'affichage. Ils ne nécessitent aucun matériel :

```bash
cargo test
```

## Tests d'intégration matériels (opt-in)

Certains comportements ne peuvent être vérifiés qu'avec un **Logitech G27
réellement branché**. Ces tests sont **désactivés par défaut** et protégés par
la feature `hardware-tests`, afin de ne jamais s'exécuter en CI ou sur une
machine sans volant.

### Prérequis

- Un **G27 branché** en USB.
- **Accès HID au périphérique** :
  - **Windows** : aucun pilote tiers à installer — le pilote HID natif suffit.
  - **Linux** : accès `hidraw` (règle `udev` dédiée, voir l'annexe du `README.md`
    racine, ou exécution privilégiée selon la configuration).

### Lancement

```bash
cargo test --features hardware-tests
```

### Couverture

| Test | Vérifie | Effet matériel |
| --- | --- | --- |
| `hid::hardware_tests::detects_a_connected_g27` | Le G27 est bien énuméré et reconnu | Lecture seule (aucun écrit) |

### Vérifié manuellement (non automatisé)

La **bascule effective** modifie l'état du volant (déconnexion / reconnexion
sous un autre Product ID). Elle n'est donc pas exécutée par la suite de tests
automatique. Pour la valider à la main :

```bash
# Mode courant
cargo run -- status

# Simulation (n'envoie rien)
cargo run -- switch --dry-run

# Bascule réelle vers le mode natif (règle aussi l'angle sur 900°)
cargo run -- switch

# Vérifier que le mode a changé
cargo run -- status

# Régler l'angle de rotation (mode natif requis), 40–900°
cargo run -- set-range 540

# Désactiver l'autocentrage matériel (mode natif requis)
cargo run -- set-autocenter off
```

> 🎯 Vérification de l'angle sous Windows : `joy.cpl` → propriétés du volant.
> À 900°, une rotation complète correspond à **2,5 tours** de volant.
>
> 🎮 Vérification de l'autocentrage : dans un jeu avec FFB (ETS2, Forza…), le
> volant ne doit **plus résister au centre** une fois l'autocentrage désactivé —
> seul le retour de force du jeu agit.

> ⚠️ La commande `switch` envoie réellement le magic packet : le volant se
> déconnecte puis réapparaît en mode natif. C'est l'effet attendu, mais il
> change l'état du périphérique.
