# chem-integrations

La commande `chem`, avec les mêmes options et les mêmes résultats que celle du moteur, construite sur quatre bibliothèques de l'écosystème Rust. Le moteur (`engine/`) n'a aucune dépendance : chaque service y est décrit par une interface, avec une implémentation de la bibliothèque standard ; ce paquet en fournit une autre par bibliothèque.

| Fonctionnalité Cargo | Module | Interface du moteur | Apport |
|---|---|---|---|
| `rayon` | `parallelisme` | `ordonnanceur::FabriqueParallelisme` | pool à vol de tâches, threads créés une fois par exécution |
| `clap` | `arguments` | `cli::arguments::AnalyseurArguments` | analyse des arguments par Clap, construite depuis les tables de la commande |
| `serde` | `trace` | `trace::FormatTrace` | lignes de trace écrites et lues par `serde_json`, identiques octet pour octet |
| `miette` | `diagnostics` | `diagnostic::RenduDiagnostic` | rendu graphique des erreurs et avertissements |

Toutes sont actives par défaut. Chacune s'active seule ; un point non activé garde l'implémentation de la bibliothèque standard.

```sh
cargo build --release                                              # integrations/target/release/chem
cargo build --release --no-default-features --features rayon,miette
cargo install --path .                                             # installe la commande chem
chem --version                                                     # implémentation de chaque point
```

Mesures du pool Rayon face au pool standard, sur les programmes du banc du moteur : `cargo bench --bench pools` (ou `scripts/chemxpe02.sh` depuis la racine), sur une machine de 4 cœurs au moins ; résultats et protocole dans [`docs/performance.md`](../docs/performance.md).

Les tests (`cargo test`) vérifient que chaque implémentation se comporte comme celle de la bibliothèque : mêmes arguments analysés, mêmes lignes de trace et même rejeu, mêmes solutions finales, diagnostics complets. L'intégration continue les lance avec chaque fonctionnalité seule.

Architecture, contrat de chaque interface et marche à suivre pour en ajouter une : [`docs/architecture.md`](../docs/architecture.md), section 5.
