# Architecture

Ce document décrit l'organisation du moteur (Rust) et du visualiseur (React, TypeScript, D3) et les arguments de correction du parallélisme.

## 1. Vue d'ensemble

```
source .chem ─► lexer ─► parser ─► AST ─► semantique ─► compilation
                                          (passes)      (regles Arc, solution initiale)
                                                               │
                         ┌─────────────────────────────────────┘
                                         ▼
                   ordonnanceur ──── tours, sous-solutions et lots en parallele (pool)
                         │
                                         ▼
                      moteur ───── reaction d'un lot (recherche, filtrage, produits)
                         │
                                         ▼  
                    evenements
                     Diffuseur ──► TraceJsonl ─► fichier .jsonl ─► visualiseur web
                                ├► TraceConsole / PasAPas (console)
                                └► Enregistreur (tests) ; ModeleTrace ◄─ rejeu
```

Le dépôt contient deux parties indépendantes : `engine/` (crate `chem` : bibliothèque + exécutable) et `visualizer/` (application web). Leur seul contrat est le format de trace (`docs/format-trace.md`) ; le moteur ne dépend pas du visualiseur.

## 2. Modules du moteur

| Module | Responsabilité (unique) |
|---|---|
| `source` | texte source, positions (ligne, colonne) |
| `diagnostic` | erreurs et avertissements localisés, rendu avec extrait souligné |
| `lexer` | texte → jetons (itérateur) |
| `ast` | arbre syntaxique, visiteur, afficheur (AST → texte) |
| `parser` | jetons → AST (descente récursive, profondeur bornée) |
| `semantique` | passes de vérification (dont les règles sans réactif, W0303) ; compilation en règles et solution initiale |
| `evaluateur` | valeurs ; expressions compilées et leur évaluation |
| `solution` | solutions imbriquées, molécules, identifiants, monteur, forme canonique, boîtes aux lettres (injection asynchrone) |
| `moteur` | règles compilées, filtrage, réaction d'un lot, garde-fous, compteurs |
| `ordonnanceur` | configuration, pool de workers, tours, répartition en lots, statistiques |
| `trace` | événements, JSON, observateurs (JSONL, console, pas-à-pas), modèle reconstruit |
| `rejeu` | relecture et vérification d'une trace |
| `application` | façade : charger, vérifier, exécuter |
| `cli` | arguments, commandes, codes de retour, serveur de `chem viz` |
| `alea`, `annulation` | générateur pseudo-aléatoire déterministe ; jeton d'arrêt partagé |

Dépendances : `cli → application → {parser, semantique, ordonnanceur}` ; `ordonnanceur → moteur → {solution, evaluateur, trace}` ; `rejeu → {trace, moteur}`. Il n'y a pas de dépendance vers le haut : `moteur` ignore `ordonnanceur`, `trace` ignore `moteur`.

## 3. Moteur d'exécution

### 3.1 Réaction d'un lot (`moteur::Reacteur`)

Un *lot* est une portion d'une solution confiée à un seul worker ; il **possède** ses éléments. La recherche procède par passes : pour chaque élément vivant `i` (l'ancre), chaque règle est essayée avec des partenaires d'indice `> i`, dans toutes les permutations (arité 2 : `(i, j)` puis `(j, i)` ; arité ≥ 3 : combinaisons puis permutations). Un pré-filtre de type écarte la plupart des couples avant toute liaison.

Protocole d'une réaction : filtrer (lier les variables), évaluer la condition, évaluer les produits, puis **réserver**, dans cet ordre : l'activation d'une règle à usage unique (drapeau atomique), un jeton d'étape, la fermeture des boîtes aux lettres des sous-solutions liées par `s:sol` (seulement si la solution a des références), les boîtes des sous-solutions référencées par `s:ref` ; enfin **valider** (retirer, ajouter, envoyer). Une réservation qui échoue annule les précédentes ; une erreur avant la validation laisse le lot intact ; la validation ne peut plus échouer. Les produits prennent les emplacements libérés après l'ancre, puis celui de l'ancre (aussitôt re-tentée), puis la fin du lot : ils sont vus dans la même passe sans faire grossir le vecteur (« parcours par lots »). Une passe complète sans réaction prouve l'inertie locale. Chaque passe commence par relever le courrier de la solution (les molécules reçues rejoignent le lot avant la recherche) et par tenter une fois chaque règle sans réactif, qui n'a pas d'ancre ; ses produits vont en fin de lot.

### 3.2 Solutions imbriquées (`ordonnanceur`)

`stabiliser(S, budget)` répète des tours jusqu'à l'inertie de `S` :

1. relever le courrier de `S` ; si une règle de `S` a des références, construire les **ports** de toutes les sous-solutions directes (avant l'extraction : ils désignent aussi celles qui vont réagir) ; extraire les sous-solutions actives de `S` (non inertes, ou qui ont du courrier) ;
2. en parallèle : stabiliser chaque sous-solution active (une tâche par sous-solution) **et** faire réagir `S` sur ses molécules et ses sous-solutions inertes ;
3. réintégrer les sous-solutions ;
4. si aucune n'était active, qu'aucune réaction n'a eu lieu et qu'aucun courrier n'attend, `S` est inerte. Les activations à usage unique qui ont réagi sont retirées de `S`.

Les deux branches de l'étape 2 travaillent sur des données disjointes (les actives ont été retirées de `S`) : une sous-solution ne peut être ni liée, ni dissoute, ni recevoir une injection synchrone pendant qu'elle réagit. Une sous-solution qui reçoit une injection synchrone quitte le lot (`reactivees`) et sera stabilisée au tour suivant. Seule exception, voulue : l'injection asynchrone, qui passe par la boîte aux lettres.

### 3.3 Parallélisme dans une solution

La phase de `S` répartit ses éléments en lots (au plus un par worker du budget, au moins 64 éléments par lot), distribués de façon circulaire après mélange en mode aléatoire. Les lots réagissent simultanément jusqu'à leur inertie locale, puis sont fusionnés deux à deux et le traitement reprend, jusqu'à un lot unique. C'est la **réservation par lots** : la réservation atomique d'un réactif est garantie par la propriété exclusive du lot (vérifiée statiquement par Rust), sans verrou ni tentative de réservation qui échoue.

Le budget de workers est réparti de façon déterministe entre la phase de `S` et ses sous-solutions actives : le découpage ne dépend que de la structure et de la graine, jamais du minutage. À graine et nombre de workers fixés, le **résultat** est donc reproductible même en parallèle ; seuls l'entrelacement des lignes de trace, les horodatages et les identifiants des produits varient (rejouables par la trace).

### 3.4 Règles à usage unique, règles sans réactif, injection asynchrone

**Usage unique.** Pendant une phase, le `Reacteur` est partagé par tous les lots de la solution ; il porte un `AtomicBool` par activation `replace-one`. La réservation est un `compare_exchange(false, true)` : une seule réaction, tous lots confondus, peut l'obtenir ; si une réservation ultérieure (jeton, boîtes) échoue, le drapeau est relâché. Après la phase, l'ordonnanceur retire les activations dont le drapeau est levé. L'événement `reaction` porte `retiree: true`.

**Sans réactif.** Une règle dont la liste de motifs consommés est vide (aucun réactif, ou seulement des `s:ref`) est tentée une fois au début de chaque passe ; la phase d'une solution a lieu dès qu'elle a des éléments **ou** une telle règle, même si elle est vide.

**Injection asynchrone.** Chaque solution possède une `BoiteAuxLettres` (`Arc`, verrou court + indicateur atomique « du courrier attend »). Une référence `s:ref` est résolue vers un `Port` (identifiant + boîte) parmi ceux de la solution ; `inject(e, s)` dépose la molécule dans la boîte au lieu de toucher au multi-ensemble de la cible, qui peut appartenir à un autre worker. Le protocole rend exclusives, **sans attente**, les deux opérations qui peuvent viser une même sous-solution inerte depuis deux lots :

| Opération | Effet | Échoue si |
|---|---|---|
| `reserver_envoi` puis `deposer` / `annuler_envoi` (expéditeur) | compte un envoi en cours, puis dépose les molécules | la boîte est fermée (sous-solution liée, dissoute ou détruite) |
| `fermer_si_libre` puis `rouvrir` (règle qui lie `s:sol`) | ferme la boîte ; rouverte après la validation si la sous-solution survit (conservation, injection synchrone) | du courrier attend ou un envoi est en cours : la sous-solution n'est pas inerte |

Ordre causal de la trace : la réaction et ses événements `envoi` sont diffusés **avant** le dépôt (une `reception` ne peut donc pas les précéder), et une réaction qui conserve une sous-solution est diffusée **avant** de rouvrir sa boîte (aucun `envoi` vers elle ne peut la précéder). Après un arrêt, le courrier encore en transit est remis à ses destinataires.

### 3.5 Pool de workers

`ordonnanceur::Pool` offre `joindre(a, b)` et `pour_chaque_mut` sur la bibliothèque standard (`std::thread::scope`). Une tâche n'est confiée à un nouveau thread que si une place de worker est libre (prise non bloquante) ; sinon elle s'exécute sur le thread courant. Il y a donc au plus `N` threads actifs, aucune attente de place, et aucun cycle d'attente possible : pas d'interblocage.

### 3.6 Arguments de correction

- **Pas de molécule consommée deux fois.** Un élément appartient à un seul lot à la fois ; le lot est un `&mut` exclusif pendant sa réaction.
- **Tout résultat parallèle est un résultat séquentiel possible.** Chaque réaction d'un lot est valide dans la solution entière (ses réactifs y sont présents, distincts, et satisfont motifs et condition) ; des réactions de lots différents portent sur des éléments disjoints et commutent. L'ordre des lignes de la trace est une exécution séquentielle valide : `chem replay --programme` la ré-exécute et le vérifie (tests de concurrence).
- **Terminaison sans faux positif.** L'inertie d'une solution n'est déclarée qu'après un tour sans sous-solution active et dont la phase n'a fait aucune réaction ; la phase se termine par une passe exhaustive sur un lot unique. Une injection rend la cible active, ce qui empêche l'inertie du parent jusqu'au tour suivant.
- **Usage unique exact.** Une activation ne réagit qu'une fois : le drapeau est pris par `compare_exchange` avant la validation et n'est relâché que si la réaction est abandonnée ; le modèle de trace refuse toute réaction d'une règle qui n'est plus active.
- **Injection asynchrone sans perte.** Une molécule envoyée est soit en transit (boîte), soit reçue (multi-ensemble) : un dépôt suit toujours une réservation réussie, et une boîte fermée refuse toute réservation ; une sous-solution liée n'a donc jamais de courrier en attente, et une sous-solution dissoute ou détruite ne reçoit plus rien. Une sous-solution qui a du courrier n'est pas inerte : elle est extraite comme active au tour suivant de son parent, qui a forcément lieu puisque l'envoi était une réaction du parent. L'inertie reste exacte. Les échecs de réservation transitoires (deux lots concurrents) ne peuvent pas fausser l'inertie : la dernière exécution de la phase porte sur un lot unique, sans concurrence.
- **Arrêts propres.** Le budget d'étapes est prélevé par blocs atomiques (limite exacte avec un worker) ; échéance, annulation et limite de taille sont contrôlées périodiquement ; la première cause d'arrêt est conservée et tous les workers s'arrêtent au point de contrôle suivant, en restituant leurs éléments : la solution affichée à l'arrêt ne perd aucune molécule.

## 4. Visualiseur

```
trace .jsonl ─► analyseur (types, erreurs par ligne)
             ─► Historique : Commande[] (appliquer / annuler) + memento tous les 256 pas
             ─► Statistiques (courbe, cumuls par regle / worker, frise)
             ─► Lecteur (Etat : pause / lecture / termine ; Observateur)
             ─► composants React + D3 (scene, courbe, barres, frise, tableau)
```

Chaque événement de trace devient une commande annulable, y compris `envoi` (molécules mises en transit), `reception` (molécules en transit rejointes au multi-ensemble) et le retrait d'une règle à usage unique (le tableau de règles est remplacé, et restauré en arrière). Les molécules en transit sont dessinées avec un contour pointillé sur le bord inférieur de leur solution destinataire ; à la réception, elles rejoignent le champ par une transition (même identifiant). Une solution qui attend du courrier n'est pas affichée inerte. Le modèle (`src/modele`, `src/lecteur`, `src/trace`) n'importe ni React ni D3 : il est testé directement sous Node 24 (`npm test`). La vue est découpée en composants qui ne reçoivent que des données immuables (instantané de l'état, effets de l'étape). La disposition de la scène (`src/vue/disposition.ts`) attribue à chaque molécule un emplacement stable dans une spirale de Vogel : un produit prend la place d'un réactif disparu, si bien qu'une réaction ne déplace que ce qui change. Les couleurs viennent d'une palette validée pour les daltonismes, avec un thème clair et un thème sombre, et toute information portée par la couleur l'est aussi par un texte (légende, étiquettes, tableau).

## 5. Dépendances et points d'intégration

| Bibliothèque | Remplacée par | Pour l'intégrer |
|---|---|---|
| Rayon | `ordonnanceur/pool.rs` | réécrire les quatre fonctions du `Pool` (exemple ci-dessous) ; rien d'autre ne change |
| Clap | `cli/arguments.rs` (tables déclaratives) | transformer les tables `RUN`, `CHECK`… en structures `#[derive(Parser)]` ; les `Commande` sont inchangées |
| Serde, Serde JSON | `trace/json.rs`, `trace/evenement.rs` | `#[derive(Serialize, Deserialize)]` avec `#[serde(tag = "ev")]` sur une forme miroir de `Evenement` ; `vers_json` / `depuis_json` deviennent des appels à `serde_json` |
| Miette | `diagnostic/rendu.rs` | implémenter `miette::Diagnostic` pour `Diagnostic` (code, labels, help) avec `NamedSource` ; le rendu graphique remplace `rendre` |
| Thiserror | implémentations manuelles de `Display` et `Error` | `#[derive(Error)]` sur `Diagnostic`, `ErreurEvaluation`, `ErreurRejeu` |
| Proptest | `tests/commun::pour_tout` (générateur déterministe, graine du cas en échec) | `proptest!` avec les mêmes générateurs ; gain : réduction automatique des contre-exemples |
| Criterion | `benches/moteur.rs` (harnais maison) | `criterion_group!` avec un `bench_function` par scénario |

Exemple pour Rayon (`ordonnanceur/pool.rs`) :

```rust
pub struct Pool { pool: rayon::ThreadPool, workers: usize }

impl Pool {
    pub fn new(workers: usize) -> Self {
        let workers = workers.max(1);
        let pool = rayon::ThreadPoolBuilder::new().num_threads(workers).build().expect("pool Rayon");
        Self { pool, workers }
    }
    pub fn workers(&self) -> usize { self.workers }
    pub fn indice_courant() -> usize { rayon::current_thread_index().unwrap_or(0) }
    pub fn installer<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R { self.pool.install(f) }
    pub fn joindre<A, B, RA, RB>(&self, a: A, b: B) -> (RA, RB)
    where A: FnOnce() -> RA + Send, B: FnOnce() -> RB + Send, RA: Send, RB: Send {
        rayon::join(a, b)
    }
    pub fn pour_chaque_mut<T: Send, R: Send, F: Fn(&mut T) -> R + Sync>(&self, e: &mut [T], f: &F) -> Vec<R> {
        use rayon::prelude::*;
        e.par_iter_mut().map(f).collect()
    }
}
```

Les tests de concurrence (`tests/concurrence.rs`) et le rejeu vérifié valident l'une ou l'autre implémentation.

## 6. Stratégie de test

| Niveau | Où | Contenu |
|---|---|---|
| Unitaires | `#[cfg(test)]` dans chaque module | lexer, parser (erreurs localisées, profondeur), afficheur, sémantique (chaque code), évaluateur, solutions, boîte aux lettres (protocole, concurrence), réacteur (courrier, usage unique partagé, boîtes fermées), pool, garde-fous, JSON, événements, modèle de trace |
| Programmes de référence | `tests/programmes_reference.rs` | tous les exemples × stratégies × graines × workers ; recette des trois jalons ; même graine = même exécution ; format de l'annexe C |
| Programmes invalides | `tests/programmes_invalides.rs` | 23 programmes, code et position attendus ; 5 000 entrées aléatoires sans plantage |
| CLI | `tests/cli.rs` | sous-commandes, codes de retour 0 à 3, trace, rejeu, trace falsifiée, pas-à-pas |
| Concurrence | `tests/concurrence.rs` | 100 exécutions répétées (max/min, dissolution, injection synchrone et asynchrone, injections bornées) avec délai anti-interblocage ; traces parallèles re-exécutées ; injections concurrentes ; garde-fous en parallèle |
| Usage unique et asynchrone | `tests/usage_unique_et_asynchrone.rs` | une seule réaction par activation même répartie en lots ; règles sans réactif ; références distinctes ; course entre envois et dissolutions (conservation de la somme) ; arrêt sans perte du courrier ; réception pendant l'exécution de la cible ; traces falsifiées refusées |
| Propriétés | `tests/proprietes.rs` | oracles (max, min, somme, PGCD, crible, tri) ; invariants de programmes non confluents ; aller-retour de l'afficheur sur programmes générés ; rejeu |
| Visualiseur | `visualizer/tests` (Node 24) | analyse de trace, historique (avance, recul, accès direct), reconstruction des solutions finales du moteur, lecteur à horloge factice, disposition |
