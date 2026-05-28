---
description: Récapitule l'état du projet et l'étape en cours selon le plan du CLAUDE.md
---

# Statut du projet

Lis le fichier `CLAUDE.md` à la racine et fais le point :

1. **Plan de développement** : quelles étapes du plan sont déjà réalisées,
   en cours, et à venir ? Base-toi sur l'état réel des fichiers du repo, pas
   sur des hypothèses.

2. **État du code** : nombre de modules, taille des fichiers (alerter si un
   fichier dépasse 200 lignes selon les conventions), couverture de tests
   approximative.

3. **Dépendances** : liste les crates dans `Cargo.toml` et leur version pinned
   dans `Cargo.lock`. Signale toute dépendance qui ne respecte pas les
   conventions de sécurité.

4. **Git** : branche actuelle, commits récents (5 derniers), fichiers
   modifiés non commit, état clean ou dirty.

5. **Prochaine étape recommandée** selon le plan du `CLAUDE.md`, avec un
   plan d'action concret pour les 30 prochaines minutes de travail.
