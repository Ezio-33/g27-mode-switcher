---
description: Lance fmt + clippy strict + tests, à exécuter avant tout commit
---

# Pré-commit qualité

Vérifie que le code est prêt à être commit selon les conventions du projet
(voir `CLAUDE.md`).

Étapes à exécuter, dans cet ordre, et à arrêter immédiatement si une étape
échoue :

1. **Format check** :
   ```bash
   cargo fmt --all -- --check
   ```
   Si ça échoue, lance `cargo fmt --all` et re-vérifie.

2. **Clippy strict (zero warning)** :
   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

3. **Tests unitaires** :
   ```bash
   cargo test
   ```

4. **Build release** (vérifie qu'il n'y a pas de warning release-only) :
   ```bash
   cargo build --release
   ```

Si toutes ces étapes passent, indiquer à l'utilisateur que le code est prêt
à être commit, et proposer un message de commit suivant la convention
Conventional Commits selon les changements effectués.
