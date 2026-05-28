# Tests

## Tests unitaires (toujours actifs)

La logique métier pure est couverte par des tests unitaires situés dans les
modules (`src/usb.rs`, `src/switcher.rs`) : classification VID/PID,
construction du magic packet, validation stricte du transfert de contrôle,
rendu d'affichage. Ils ne nécessitent aucun matériel :

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
- **Accès USB au périphérique** :
  - **Windows** : pilote **WinUSB** posé sur l'interface du G27 via
    [Zadig](https://zadig.akeo.ie/) (voir le `README.md` racine).
  - **Linux** : droits suffisants pour ouvrir le périphérique (règle `udev`
    dédiée, ou exécution privilégiée selon la configuration).

### Lancement

```bash
cargo test --features hardware-tests
```

### Couverture

| Test | Vérifie | Effet matériel |
| --- | --- | --- |
| `usb::hardware_tests::detects_a_connected_g27` | Le G27 est bien énuméré et reconnu | Lecture seule (aucun écrit) |

### Vérifié manuellement (non automatisé)

La **bascule effective** modifie l'état du volant (déconnexion / reconnexion
sous un autre Product ID). Elle n'est donc pas exécutée par la suite de tests
automatique. Pour la valider à la main :

```bash
# Mode courant
cargo run -- status

# Simulation (n'envoie rien)
cargo run -- switch --dry-run

# Bascule réelle vers le mode natif
cargo run -- switch

# Vérifier que le mode a changé
cargo run -- status
```

> ⚠️ La commande `switch` envoie réellement le magic packet : le volant se
> déconnecte puis réapparaît en mode natif. C'est l'effet attendu, mais il
> change l'état du périphérique.
