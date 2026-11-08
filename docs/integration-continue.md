# Intégration continue

Les workflows GitHub Actions de `.github/workflows/` vérifient chaque poussée sur `main` et chaque demande de fusion. Ils reprennent les scripts de `scripts/`, qui restent la référence en local : un échec en ligne se reproduit avec la même commande sur son poste.

## Workflows

| Workflow | Fichier | Déclenchement | Ce qu'il vérifie ou produit |
|---|---|---|---|
| Intégration continue | `ci.yml` | poussée sur `main`, demande de fusion, manuel | moteur sous Linux, macOS et Windows (rustfmt, clippy, tests, rustdoc ; sous Linux, traces de démonstration rejouées) ; paquet `integrations` avec toutes ses fonctionnalités, aucune, puis chacune seule ; tests de concurrence répétés avec les deux exécutables ; visualiseur (types, tests, construction) ; scripts (syntaxe, ShellCheck, ni tabulation ni caractère accentué) |
| Conventions | `conventions.yml` | ouverture ou modification d'une demande de fusion | titre au format Conventional Commits (`feat(moteur): ...`), comme l'historique du dépôt |
| Couverture | `couverture.yml` | poussée sur `main`, demande de fusion touchant `engine/`, manuel | couverture des tests du moteur (`cargo llvm-cov`) : résumé dans le récapitulatif du job, rapport HTML et fichier LCOV en artefact |
| Sécurité | `securite.yml` | poussée sur `main`, demande de fusion, chaque lundi, manuel | failles connues des dépendances Rust (`cargo audit`, base RustSec) et npm (`npm audit`) ; analyse CodeQL du Rust, du TypeScript et des workflows ; revue des dépendances ajoutées par une demande de fusion |
| Performance | `performance.yml` | demande de fusion touchant le moteur ou le pool Rayon, manuel (choix de la machine) | `scripts/chemxpe01.sh` sur la branche et sur la base, tableaux côte à côte ; pool Rayon face au pool standard (`scripts/chemxpe02.sh`) sur une machine de 4 cœurs, tableau dans le récapitulatif et en artefact (`mesures-pools`, 90 jours) |
| Visualiseur en ligne | `pages.yml` | poussée sur `main` touchant `visualizer/`, manuel | visualiseur construit, avec ses traces de démonstration, publié sur GitHub Pages |
| Publication | `publication.yml` | étiquette `v*`, manuel | commande `chem` (paquet `integrations`) pour Linux x86-64, macOS ARM et Windows x86-64, avec les exemples et la documentation, visualiseur construit, sommes SHA-256, release GitHub (lancé à la main : archives en artefacts, sans release) |

Dependabot (`.github/dependabot.yml`) propose chaque lundi une demande de fusion groupée par écosystème : actions GitHub, dépendances du paquet `integrations`, dépendances du visualiseur. Le moteur n'en a aucune.

## Équivalents locaux

| Vérification en ligne | En local |
|---|---|
| format, clippy et tests du moteur, du paquet `integrations` et du visualiseur | `scripts/chemxte01.sh` |
| tests de concurrence répétés | `scripts/chemxte02.sh 25 4` (`CHEM_BINAIRE=integrations/target/release/chem` pour l'exécutable du paquet) |
| traces de démonstration rejouées | `scripts/chemxvi02.sh` |
| couverture | `scripts/chemxte03.sh` |
| mesures de performance | `scripts/chemxpe01.sh` |
| pool Rayon face au pool standard | `scripts/chemxpe02.sh` (4 cœurs au moins) |
| une fonctionnalité du paquet seule | `cargo test --manifest-path integrations/Cargo.toml --no-default-features --features serde` |

## Principes

- **Une seule version de Rust.** L'action locale `.github/actions/rust` lit le canal dans `rust-toolchain.toml`, installe cette chaîne avec rustfmt et clippy, puis restaure le cache Cargo du moteur et du paquet `integrations`. Changer de version de Rust ne demande de modifier que `rust-toolchain.toml`.
- **Moindre privilège.** Chaque workflow n'a que la lecture du dépôt (`contents: read`) ; seuls les jobs qui en ont besoin reçoivent davantage (résultats CodeQL, déploiement Pages, création de la release). Le jeton n'est pas conservé après la récupération du code (`persist-credentials: false`).
- **Pas de travail inutile.** Une nouvelle poussée sur une demande de fusion annule la vérification en cours ; la couverture et les mesures ne tournent que si le moteur change ; Pages ne se reconstruit que si le visualiseur change.
- **Chaque point d'intégration seul.** La matrice du job `integrations` construit et teste le paquet avec chaque bibliothèque seule : aucun module ne peut dépendre d'un autre sans que l'intégration continue échoue.

## À activer une fois

1. **Pages** : Settings > Pages > Source : *GitHub Actions*. Tant que Pages n'est pas activé, le workflow `pages.yml` est sauté au lieu d'échouer.
2. **Règles de la branche `main`** : Settings > Rules > Rulesets > New ruleset > *Import a ruleset*, avec le fichier `.github/rulesets/protection-main.json`. Il interdit la suppression et la réécriture de `main`, impose une demande de fusion (fusion par squash ou rebase, discussions résolues) et le succès des vérifications principales : `Moteur (ubuntu-latest)`, `Integrations (toutes)`, `Visualiseur`, `Scripts shell` et `Titre de la demande de fusion`. Les administrateurs du dépôt peuvent passer outre.
3. **Analyse de code** : CodeQL est configuré par `securite.yml` (configuration avancée) ; la configuration par défaut de Settings > Code security doit rester désactivée.
4. **Cargo.lock du paquet `integrations`** : il est créé au premier `cargo build` du paquet ; le versionner fige les versions de Rayon, Clap, Serde et Miette et permet à Dependabot de les mettre à jour.

## Publier une version

```sh
git tag v1.2.0
git push origin v1.2.0
```

Le workflow `publication.yml` construit les archives, vérifie l'exécutable sur chaque plateforme (`chem --version`, un exemple) et crée la release avec des notes générées à partir des demandes de fusion.
