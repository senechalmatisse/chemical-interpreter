# Manuel utilisateur

## Installation

Prérequis : Rust 1.99.1 (fixé par `rust-toolchain.toml`, installé automatiquement par rustup) et Node.js 24 LTS pour le visualiseur.

```sh
cd engine
cargo build --release          # binaire : engine/target/release/chem
cargo test

cd ../integrations             # facultatif : la même commande avec Rayon, Clap, Serde et Miette
cargo build --release          # binaire : integrations/target/release/chem

cd ../visualizer
npm install                    # crée package-lock.json au premier passage
npm run typecheck && npm test && npm run build
```

Le moteur n'a **aucune dépendance externe** : il se construit hors ligne. Le paquet `integrations/` construit la même commande `chem`, avec les mêmes options et les mêmes résultats, en s'appuyant sur Rayon (pool de workers à vol de tâches), Clap (analyse des arguments), Serde (trace JSON Lines) et Miette (rendu graphique des erreurs) ; il télécharge ses dépendances au premier `cargo build`. Chaque bibliothèque est une fonctionnalité Cargo que l'on peut désactiver (`--no-default-features --features rayon,miette`) ; `chem --version` indique l'implémentation utilisée pour chaque point d'intégration (détails : `docs/architecture.md`, section 5).

```console
$ chem --version
chem 0.1.0
  arguments    : clap
  diagnostics  : miette
  trace        : serde_json
  parallelisme : rayon
```

## Commandes

```
chem <commande> [options]
```

| Commande | Rôle |
|---|---|
| `chem run <prog.chem>` | exécute jusqu'à l'inertie et affiche la solution finale |
| `chem check <prog.chem>` | vérifie sans exécuter (syntaxe, sémantique, solution initiale) |
| `chem fmt <prog.chem>` | réimprime le programme sous forme normalisée |
| `chem replay <trace.jsonl> [--programme prog.chem]` | rejoue une trace (et vérifie chaque réaction) |
| `chem viz <trace.jsonl>` | ouvre le visualiseur web sur la trace |

`chem --help` et `chem <commande> --help` décrivent toutes les options.

### Options de `chem run`

| Option | Effet |
|---|---|
| `--strategie aleatoire\|ordonnee` | ordre de sélection des réactions (défaut : `aleatoire`) |
| `--seed N` | graine du générateur ; sans graine, elle est tirée de l'horloge et affichée par `--stats` et dans la trace |
| `--workers N` | nombre de workers (défaut 1 = séquentiel) |
| `--max-etapes N` | arrêt après N réactions |
| `--delai-max D` | arrêt après une durée : `5`, `1.5s`, `500ms`, `2m` |
| `--max-molecules N\|aucune` | taille maximale (défaut 10 000 000) |
| `--trace` | affiche la solution après chaque étape |
| `--trace-jsonl FICHIER` | écrit la trace JSON Lines (`-` : sortie standard) |
| `--pas-a-pas` | exécution interactive : Entrée = étape suivante, `c` = continuer, `q` = arrêter (force 1 worker) |
| `--stats` | statistiques : réactions par règle et par worker, tailles, passes, durée ; molécules envoyées et reçues (injection asynchrone) et règles à usage unique retirées, si le programme en a ; recherches de partenaires par valeur (conditions d'égalité) ; phases réparties d'après une jointure, phases terminées sans fusion et segments de chaîne |
| `--sans-index` | désactive l'index par valeur des conditions d'égalité entre réactifs (comparaison, diagnostic) : seul l'ordre d'essai des partenaires change, les résultats possibles restent les mêmes ; un programme confluent donne exactement le même résultat |
| `--sans-partition` | désactive la répartition des lots guidée par les jointures (par clé pour `x == y` ou `k == l`, par segments pour une chaîne comme `b == c`) : distribution circulaire, comme les autres règles ; même remarque sur les résultats |

La solution finale s'affiche sous forme **canonique** (valeurs triées, sous-solutions triées, puis règles) : deux exécutions de même résultat affichent le même texte.

```console
$ chem run exemples/b06_dissolution.chem --workers 4 --stats
{ 21 | vider, max }
statistiques :
  graine         : 1312247302478593896 (strategie aleatoire, 4 worker(s))
  reactions      : 10
  par regle      : max = 7, vider = 3
  ...
```

### Codes de retour

| Code | Cas |
|---|---|
| 0 | succès (solution inerte, ou commande réussie) |
| 1 | erreur de syntaxe ou de sémantique (ou ligne de commande invalide, fichier illisible) |
| 2 | erreur d'exécution (division par zéro, dépassement, type) ou d'écriture de la trace |
| 3 | garde-fou atteint (`--max-etapes`, `--delai-max`, `--max-molecules`) ou interruption en pas-à-pas |

En cas d'arrêt (codes 2 et 3), la solution au moment de l'arrêt est tout de même affichée.

## Visualisation console

```console
$ chem run exemples/b01_maximum.chem --strategie ordonnee --trace
etape 0 (etat initial)
    { 2, 4, 9, 17, 17 | max }
etape 1 : max dans s0 (worker 0) : 17, 4 -> 17
    { 2, 9, 17, 17 | max }
...
```

Les règles à usage unique et l'injection asynchrone apparaissent ainsi (programme `b20_injection_bornee.chem`) : la règle retirée, l'envoi, les molécules en transit puis leur réception.

```console
$ chem run exemples/b20_injection_bornee.chem --strategie ordonnee --trace
...
etape 3 : alimenter dans s0 (worker 0) : rien -> rien
    regle 'alimenter' retiree de s0 (usage unique)
    envoi asynchrone de 50, 8 vers s1
    { { 17 | max } }
    en transit vers s1 : 50, 8
reception dans s1 : 50, 8
    { { 8, 17, 50 | max } }
etape 4 : max dans s1 (worker 0) : 50, 17 -> 50
...
```

## Visualiseur web

1. Produire une trace :
   ```sh
   chem run prog.chem --workers 4 --trace-jsonl trace.jsonl
   ```

2. Construire le visualiseur une fois :
   ```sh
   cd visualizer
   npm install
   npm run typecheck && npm test && npm run build
   ```

3. Lancer le visualiseur depuis le répertoire `visualizer` :
   ```sh
   chem viz public/exemples/b06_dissolution_w4.jsonl --dist dist
   ```

L'option `--dist dist` indique explicitement au moteur où trouver les fichiers compilés du visualiseur. Elle est nécessaire lorsque le chemin par défaut `visualizer/dist` ne correspond pas au répertoire courant.

   Pour visualiser une autre trace :
   ```sh
   chem viz ../trace.jsonl --dist dist
   ```

On peut également lancer la commande depuis la racine du dépôt, en adaptant les chemins :
   ```sh
   chem viz trace.jsonl --dist visualizer/dist
   ```

Un serveur local sert le visualiseur et la trace, puis ouvre le navigateur.

On peut aussi lancer `npm run dev` et charger une trace par le bouton « Ouvrir une trace… », par glisser-déposer, ou choisir une des démonstrations fournies (crible jusqu'à 100, B6 avec 4 workers, règles à usage unique, crible alimenté par injection asynchrone…).

| Élément | Rôle |
|---|---|
| Scène centrale | sous-solutions en conteneurs imbriqués (bord bleu : active, gris : inerte), molécules avec leur valeur ; les réactifs passent au rouge puis disparaissent, les produits apparaissent en vert à leur place ; les molécules **en transit** (injection asynchrone) attendent, contour pointillé, sur le bord de leur solution destinataire et la rejoignent à leur réception ; l'étiquette d'une solution liste ses règles actives (une règle à usage unique en disparaît quand elle a réagi) ; molette pour zoomer, glisser pour déplacer |
| Contrôles | début, étape précédente (retour arrière), lecture/pause, étape suivante, fin, curseur de position, vitesse de 0,5 à 256 étapes/s |
| Clavier | espace : lecture/pause ; ← → : étape ; Origine/Fin |
| Molécules par étape | courbe ; survol : valeur ; clic : aller à l'étape |
| Réactions par règle / par worker | cumul jusqu'à l'étape courante sur le total de la trace |
| Frise par worker | activité de chaque worker en temps réel ; les événements étant écrits par lots, l'ordre de relecture (celui du fichier) peut différer de l'ordre chronologique entre workers |
| Étape courante, Exécution | description de l'événement courant, nombre de molécules (dont en transit) ; paramètres de l'exécution, envois asynchrones et règles retirées |
| Tableau des réactions | mêmes comptes sous forme de tableau (accessibilité) |
| Exporter SVG / PNG | image de la scène courante |

## Scripts

Les scripts de `scripts/` suivent la norme de nommage `<aaaafddnn.sh>` (application `chem`, fréquence `x` = à la demande) :

| Script | Rôle |
|---|---|
| `chemxte01.sh` | chaîne de qualité complète : rustfmt, clippy, tests du moteur, du paquet `integrations` (si ses dépendances sont disponibles) et du visualiseur, vérification de types |
| `chemxte02.sh [N] [workers]` | N exécutions répétées (défaut 100) des programmes à sous-solutions, dissolution, injection synchrone et asynchrone, règles à usage unique, avec vérification des résultats et des traces |
| `chemxpe01.sh [fichier]` | mesures de performance (bancs du moteur) et rapport Markdown |
| `chemxvi01.sh` | régénère les traces de démonstration du visualiseur |
| `chemxvi02.sh` | rejoue les traces de démonstration et compare leur solution finale au résultat attendu de l'index |
| `chemxpe02.sh` | mesure le pool Rayon face au pool standard (paquet `integrations`, 4 cœurs au moins) |
| `chemxte03.sh` | rapport de couverture (nécessite `cargo-llvm-cov`) |

La variable d'environnement `CHEM_BINAIRE` choisit l'exécutable des scripts `chemxte02.sh`, `chemxvi01.sh` et `chemxvi02.sh` : `CHEM_BINAIRE=integrations/target/release/chem scripts/chemxte02.sh` rejoue les tests de concurrence avec Rayon.

## Intégration continue

Les workflows GitHub Actions de `.github/workflows/` reprennent ces scripts à chaque poussée et à chaque demande de fusion : voir `docs/integration-continue.md`.
