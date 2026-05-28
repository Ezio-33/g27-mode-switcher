---
description: Cross-compile le projet en .exe Windows et place le binaire dans dist/
---

# Build Windows release

Compile une version release pour Windows x86_64 (cross-compile depuis Linux),
copie le binaire dans `dist/` à la racine du projet pour faciliter le transfert
vers le PC gaming.

Étapes à exécuter :

1. Vérifier que la target `x86_64-pc-windows-gnu` est installée :
   ```bash
   rustup target list --installed | grep -q "x86_64-pc-windows-gnu" || \
     rustup target add x86_64-pc-windows-gnu
   ```

2. Lancer le build release cross-compile :
   ```bash
   cargo build --release --target x86_64-pc-windows-gnu
   ```

3. Créer le dossier `dist/` s'il n'existe pas et copier le binaire :
   ```bash
   mkdir -p dist
   cp target/x86_64-pc-windows-gnu/release/g27-mode-switcher.exe dist/
   ls -lh dist/g27-mode-switcher.exe
   ```

4. Afficher la taille finale du binaire et le chemin pour copier sur le PC
   gaming (via NAS, clé USB, etc.).

5. Rappeler à l'utilisateur de copier également la `libusb-1.0.dll` à côté du
   `.exe` si nécessaire (à vérifier selon la stratégie de linkage choisie).
