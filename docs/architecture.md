# Architecture

Ce document décrit l'organisation du moteur (Rust), du paquet `integrations` et du visualiseur (React, TypeScript, D3), et les arguments de correction du parallélisme.

## 1. Vue d'ensemble

```
source .chem ─► lexer ─► parser ─► AST ─► semantique ─► compilation
                                            (passes) (regles Arc, solution initiale)
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

Le dépôt contient trois parties : `engine/` (crate `chem` : bibliothèque + exécutable, sans aucune dépendance), `integrations/` (paquet `chem-integrations` : les points d'intégration du moteur branchés sur Rayon, Clap, Serde et Miette, et une commande `chem` construite avec eux, voir section 5) et `visualizer/` (application web). Le moteur et le visualiseur n'ont qu'un contrat, le format de trace (`docs/format-trace.md`) ; le moteur ne dépend ni du visualiseur ni du paquet `integrations`.

## 2. Modules du moteur

| Module | Responsabilité |
|---|---|
| `source` | texte source, positions (ligne, colonne) |
| `diagnostic` | erreurs et avertissements localisés ; rendu interchangeable (`RenduDiagnostic`), extrait souligné par défaut |
| `lexer` | texte → jetons (itérateur) |
| `ast` | arbre syntaxique, visiteur, afficheur (AST → texte) |
| `parser` | jetons → AST (descente récursive, profondeur bornée) |
| `semantique` | passes de vérification (dont les règles sans réactif, W0303) ; compilation en règles et solution initiale |
| `evaluateur` | valeurs ; expressions compilées et leur évaluation |
| `solution` | solutions imbriquées, molécules, identifiants, monteur, forme canonique, boîtes aux lettres (injection asynchrone) |
| `moteur` | règles compilées, filtrage, index par valeur (jointures sur égalité), réaction d'un lot, garde-fous, compteurs |
| `ordonnanceur` | configuration, interface de parallélisme (`Parallelisme`) et pool de workers, tours, répartition en lots (circulaire ou par clé), statistiques |
| `trace` | événements, JSON, codage interchangeable des lignes (`FormatTrace`), observateurs (JSONL, console, pas-à-pas), modèle reconstruit |
| `rejeu` | relecture et vérification d'une trace |
| `application` | façade : charger, vérifier, exécuter |
| `cli` | arguments (analyse interchangeable, `AnalyseurArguments`), commandes, `Composants`, codes de retour, serveur de `chem viz` |
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

La phase de `S` répartit ses éléments en lots (au plus un par worker du budget, au moins 64 éléments par lot), distribués de façon circulaire après mélange en mode aléatoire, ou guidés par une jointure : par clé, et par segments pour une chaîne (section 3.6). Les lots réagissent simultanément jusqu'à leur inertie locale, puis sont fusionnés deux à deux et le traitement reprend, jusqu'à un lot unique. C'est la **réservation par lots** : la réservation atomique d'un réactif est garantie par la propriété exclusive du lot (vérifiée statiquement par Rust), sans verrou ni tentative de réservation qui échoue.

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

### 3.5 Index par valeur des conditions d'égalité (`moteur::jointure`)

Sans index, une règle à deux réactifs essaie chaque ancre avec tous les partenaires du lot : le coût d'une passe est quadratique. Or une condition comme `if b == c` dans `replace (a, b), (c, d) by (a, d) if b == c` ne peut être vraie que pour les partenaires dont la **clé** (`c`) vaut celle de l'ancre (`b`). À la compilation de la règle, `jointure::analyser` cherche une telle égalité entre une expression du premier réactif et une expression du second, puis le réacteur range les molécules du lot dans un index (table de hachage clé → emplacements) et ne tente que les candidats du bon seau.

L'égalité n'est retenue que si l'index ne peut pas changer le comportement du programme :

- la condition est une conjonction évaluée de gauche à droite avec court-circuit : les conjonctions qui précèdent l'égalité ne doivent pas pouvoir échouer, sinon une erreur d'évaluation sur un couple écarté par l'index ne serait plus signalée ;
- la clé du partenaire, calculée pour chaque molécule rangée, ne doit pas pouvoir échouer ;
- la clé de l'ancre peut échouer : le réacteur revient alors au parcours complet pour cette ancre, qui évalue la condition exactement comme sans index ;
- `==` est total (comparaison structurelle de deux valeurs quelconques) : les couples de clés égales sont exactement ceux pour lesquels l'égalité est vraie.

L'index appartient au lot (aucun partage entre workers) et se construit à la première consultation d'une passe. Il n'est pas tenu à jour à chaque retrait : une entrée dont la molécule a été consommée est reconnue à son identifiant et retirée quand on la rencontre. Les produits d'une réaction y sont rangés dès leur placement, si bien qu'une chaîne se referme dans la même passe. L'ancre ne joue que son rôle (premier ou second réactif) : l'autre ordre est essayé quand le partenaire devient ancre à son tour. `chem run --sans-index` désactive l'index (comparaison, diagnostic) et `--stats` compte les recherches indexées. Gains mesurés : `docs/performance.md`.

### 3.6 Répartition des lots guidée par les jointures (`ordonnanceur::repartition`)

L'index accélère la recherche dans un lot, mais en distribution circulaire deux molécules de même clé sont le plus souvent dans des lots différents : les lots trouvent peu de couples, et l'essentiel des réactions attend les fusions, qui sont séquentielles. Pour les jointures dont les deux réactifs ont **le même motif et la même clé** (au nom des variables près : `replace x, y by x if x == y`, `replace (k, a), (l, b) by (k, a + b) if k == l`), une molécule a la même clé dans les deux rôles, et seules deux molécules de même clé peuvent réagir. `jointure::analyser` marque ces jointures `symetrique` à la compilation ; quand une règle de la solution en a une, la phase range les éléments par clé :

1. la clé de chaque molécule est calculée en parallèle (une tranche d'éléments par lot) et réduite à une empreinte de 64 bits ; deux clés d'empreintes égales partagent un groupe, ce qui ne sépare jamais deux molécules de même clé ;
2. chaque groupe qui tient dans un lot (au plus `⌈n / lots⌉` éléments) y est placé en entier, du plus grand groupe au plus petit, dans le lot le moins chargé ;
3. les groupes plus grands qu'un lot et les éléments sans clé (sous-solutions, molécules que le motif ne filtre pas) sont distribués de façon circulaire : un très grand groupe est ainsi réduit en parallèle dans chaque lot, puis à la fusion.

Les éléments gardent leur ordre relatif dans chaque lot, et la répartition ne dépend que des valeurs et de la graine : le résultat reste reproductible. Ce n'est qu'un placement : toute répartition est correcte, puisque chaque réaction d'un lot est valide dans la solution entière et que la passe exhaustive du lot final garantit l'inertie.

**Phase sans fusion.** La jointure est en plus dite `locale` quand chaque produit garde la clé de ses réactifs ou ne peut pas être filtré par le motif : `jointure` le vérifie à la compilation en filtrant symboliquement chaque produit par le motif (`(k, a + b)` lie `l` à `k`, dont la clé est celle de l'ancre ; un scalaire ne peut pas être filtré par un motif couple), sans rien évaluer. Si la règle locale est la seule de la solution et qu'aucun groupe n'a été distribué entre plusieurs lots, aucune molécule d'un lot ne peut réagir avec une molécule d'un autre lot, ni pendant la phase ni après : des lots inertes forment une solution inerte, et la phase s'arrête sans fusion. La fusion est conservée dans trois cas où des lots concurrents partagent un état : le courrier d'une injection asynchrone, qui peut arriver dans n'importe quel lot (dès qu'un lot en a relevé) ; une règle à usage unique, dont le drapeau pris par un lot peut faire passer une réaction à un autre ; une règle à références `s:ref`, dont la réservation d'une boîte peut échouer transitoirement. Dans ces trois cas, c'est la passe du lot final, sans concurrence, qui rattrape une réaction manquée.

**Contraction parallèle des chaînes.** Dans une jointure asymétrique, la clé d'une molécule n'est pas la même dans ses deux rôles : pour `replace (a, b), (c, d) by (a, d) if b == c`, le maillon `(a, b)` est partenaire pour la clé `a` et ancre pour la clé `b`. Aucune clé unique ne réunit alors les couples, et la répartition s'appuie sur le **rôle** de chaque molécule, calculé en parallèle comme les clés :

- une molécule qui n'a qu'une clé (elle ne peut tenir qu'un des deux rôles, comme dans `replace (k, v), (l, w, x) by ... if k == l`, ou ses deux clés sont égales) rejoint le groupe de sa clé, comme pour une jointure symétrique ;
- une molécule qui a deux clés différentes est un **maillon**. Chaque maillon est relié à un successeur, le premier maillon encore libre dont la clé de partenaire vaut sa clé d'ancre, les maillons choisissant dans l'ordre des éléments : un maillon n'a jamais deux prédécesseurs, et les liens forment des chaînes et des cycles disjoints, même quand des branches se disputent un maillon ;
- les chaînes sont coupées en des **repères** : leurs débuts (maillons sans prédécesseur) et des maillons tirés au hasard avec la graine du tour, assez nombreux pour donner en moyenne 8 segments par lot (un cycle sans repère en reçoit un). Chaque segment va d'un repère au suivant ; un segment plus long qu'une demi-part de lot est coupé en morceaux contigus ;
- les segments sont placés comme les groupes, du plus grand au plus petit dans le lot le moins chargé.

**Construction parallèle des successeurs.** Le découpage se fait en quatre étapes, partagées entre les workers de la phase à partir de 131 072 maillons (en dessous, elles s'exécutent sur le thread appelant) :

1. chaque tranche de maillons les range par plage d'empreintes (les bits de poids fort de l'empreinte de chaque clé), une fois selon leur clé de partenaire et une fois selon leur clé d'ancre ;
2. chaque plage relie ses maillons avec sa propre table : un maillon et son successeur ont une clé commune, donc la même plage, et chaque plage voit ses maillons dans l'ordre des éléments, comme le passage séquentiel ;
3. chaque tranche recopie les successeurs de ses maillons dans sa partie du tableau des successeurs (de son premier maillon au premier de la tranche suivante) et choisit ses repères ;
4. chaque tranche parcourt les segments qui partent de ses repères, jusqu'à 16 de front pour que le processeur recouvre leurs lectures. Un maillon n'ayant qu'un prédécesseur, deux parcours ne se croisent jamais ; un cycle sans repère est repris ensuite, sur le thread appelant.

Aucune tâche n'écrit dans les données d'une autre : il n'y a ni verrou ni opération atomique, et les cœurs ne se disputent pas les mêmes lignes de cache. Le résultat ne dépend pas du nombre de tâches : le tirage d'un repère ne dépend que de la graine et de l'élément (`Alea::au_rang` donne directement le tirage de rang `k` de la suite d'une graine), et les segments sont rendus dans l'ordre de leurs repères. Les petites chaînes n'utilisent qu'une plage, pour ne pas payer des tables inutiles. Mesures : `docs/performance.md`.

Chaque lot contracte ses segments en parallèle : grâce à l'index et au produit retenté aussitôt à la place de l'ancre, un segment se referme en une seule passe, en une molécule. Il ne reste à la fusion que quelques molécules par lot, celles des jonctions coupées, que le lot final referme. La phase garde donc ses fusions (des couples sont à cheval sur deux lots), mais elles ne coûtent presque plus rien. `chem run --sans-partition` rétablit la distribution circulaire pour toutes les règles, et `--stats` compte les phases réparties, celles terminées sans fusion et les segments de chaîne.

### 3.7 Pool de workers

L'ordonnanceur ne connaît que l'interface `ordonnanceur::Parallelisme` (`joindre(a, b)`, `installer`, indice du worker courant), utilisable comme objet : les tâches y sont des closures `FnMut` sans résultat, et `Executeur` permet d'y passer des closures génériques qui retournent une valeur (`joindre`, `pour_chaque_mut`). Une `FabriqueParallelisme` crée le pool de chaque exécution.

L'implémentation de la bibliothèque, `ordonnanceur::Pool`, repose sur `std::thread::scope`. Une tâche n'est confiée à un nouveau thread que si une place de worker est libre (prise non bloquante) ; sinon elle s'exécute sur le thread courant. Il y a donc au plus `N` threads actifs, aucune attente de place, et aucun cycle d'attente possible : pas d'interblocage. Le paquet `integrations` en fournit une autre avec Rayon (section 5).

### 3.8 Arguments de correction

- **Pas de molécule consommée deux fois.** Un élément appartient à un seul lot à la fois ; le lot est un `&mut` exclusif pendant sa réaction.
- **Tout résultat parallèle est un résultat séquentiel possible.** Chaque réaction d'un lot est valide dans la solution entière (ses réactifs y sont présents, distincts, et satisfont motifs et condition) ; des réactions de lots différents portent sur des éléments disjoints et commutent. L'ordre des lignes de la trace est une exécution séquentielle valide : `chem replay --programme` la ré-exécute et le vérifie (tests de concurrence).
- **Terminaison sans faux positif.** L'inertie d'une solution n'est déclarée qu'après un tour sans sous-solution active et dont la phase n'a fait aucune réaction ; la phase se termine par une passe exhaustive sur un lot unique, ou sur des lots indépendants qu'aucune réaction ne peut réunir (section 3.6). Une injection rend la cible active, ce qui empêche l'inertie du parent jusqu'au tour suivant.
- **Usage unique exact.** Une activation ne réagit qu'une fois : le drapeau est pris par `compare_exchange` avant la validation et n'est relâché que si la réaction est abandonnée ; le modèle de trace refuse toute réaction d'une règle qui n'est plus active.
- **Injection asynchrone sans perte.** Une molécule envoyée est soit en transit (boîte), soit reçue (multi-ensemble) : un dépôt suit toujours une réservation réussie, et une boîte fermée refuse toute réservation ; une sous-solution liée n'a donc jamais de courrier en attente, et une sous-solution dissoute ou détruite ne reçoit plus rien. Une sous-solution qui a du courrier n'est pas inerte : elle est extraite comme active au tour suivant de son parent, qui a forcément lieu puisque l'envoi était une réaction du parent. L'inertie reste exacte. Les échecs de réservation transitoires (deux lots concurrents) ne peuvent pas fausser l'inertie : la dernière exécution de la phase porte sur un lot unique, sans concurrence (une règle à références n'a jamais de lots indépendants).
- **Index sans changement de sémantique.** Une égalité n'est indexée que si l'écart des couples de clés différentes ne peut ni masquer une erreur ni écarter un couple qui aurait réagi (section 3.5) : les réactions possibles sont exactement les mêmes, seul l'ordre d'essai des partenaires change. Les tests comparent résultats (programmes confluents), erreurs et traces avec et sans index.
- **Répartition guidée sans changement de sémantique.** La répartition, par clé ou par segments, ne change que le placement des éléments entre les lots ; une phase ne se termine sans fusion que si aucune réaction ne peut réunir deux lots (une clé par lot, seule règle, jointure locale, aucun courrier), ce qui fait d'une union de lots inertes une solution inerte (section 3.6). Les tests comparent les résultats et vérifient les traces avec et sans répartition, et contrôlent dans quels cas la fusion est évitée.
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

Le moteur n'a **aucune dépendance** : il se construit hors ligne et sa chaîne de dépendances ne peut pas casser. Chaque service qu'une bibliothèque de l'écosystème rendrait mieux y est décrit par une **interface** utilisable comme objet (`dyn`), avec une implémentation de la bibliothèque standard. Le paquet `integrations/` fournit une implémentation par bibliothèque, chacune dans son module et derrière sa propre fonctionnalité Cargo ; la ligne de commande reçoit ses implémentations réunies dans `cli::Composants`.

| Point d'intégration | Interface du moteur | Implémentation standard | Paquet `integrations` | Fonctionnalité |
|---|---|---|---|---|
| Pool de workers | `ordonnanceur::FabriqueParallelisme`, `Parallelisme` | `ParallelismeStandard` (`Pool`, `std::thread::scope`) | `parallelisme::FabriqueRayon` (`rayon::ThreadPool`, vol de tâches) | `rayon` |
| Analyse des arguments | `cli::arguments::AnalyseurArguments` | `AnalyseurInterne` | `arguments::AnalyseurClap` (commandes Clap construites depuis les tables `RUN`, `CHECK`…) | `clap` |
| Codage de la trace | `trace::FormatTrace` | `FormatInterne` (`trace::json`) | `trace::FormatSerde` (`Serialize` et `Deserialize` des valeurs JSON, `serde_json`) | `serde` |
| Rendu des diagnostics | `diagnostic::RenduDiagnostic` | `RenduTexte` (extrait souligné) | `diagnostics::RenduMiette` (rendu graphique de Miette) | `miette` |

```
              chem (engine/, aucune dépendance)                     chem-integrations (integrations/)
  ┌───────────────────────────────────────────────┐         ┌─────────────────────────────────────┐
  │ cli::Composants ─┬─► AnalyseurArguments ◄─────┼─ clap ──┤ arguments::AnalyseurClap            │
  │                  ├─► RenduDiagnostic ◄────────┼─ miette ┤ diagnostics::RenduMiette            │
  │                  ├─► FormatTrace ◄────────────┼─ serde ─┤ trace::FormatSerde                  │
  │                  └─► FabriqueParallelisme ◄───┼─ rayon ─┤ parallelisme::FabriqueRayon         │
  │ Composants::standard() : implémentations std  │         │ composants() : fonctionnalités      │
  └───────────────────────────────────────────────┘         │ actives, std pour les autres        │
                                                            └─────────────────────────────────────┘
```

Chaque implémentation garde le comportement de la bibliothèque, et `integrations/tests/equivalence.rs` le vérifie :

- **Rayon.** Un `ThreadPool` est créé par exécution, avec la pile des workers standard (`PILE_WORKER`). Le contrat de `Parallelisme` est respecté : `joindre` attend les deux tâches et propage leurs paniques ; avec un seul worker, `a` puis `b` s'exécutent sur l'unique thread (même trace, hors instants, que le pool standard) ; l'indice du worker est l'indice Rayon du thread. Les exemples donnent les résultats attendus avec 1, 2 et 4 workers. `integrations/benches/pools.rs` (`scripts/chemxpe02.sh`, job « Pool Rayon face au pool standard » du workflow Performance) compare les deux pools sur les programmes du banc du moteur ; voir `docs/performance.md`.
- **Clap.** Les commandes Clap sont construites à partir des tables déclaratives de `cli` : la ligne de commande n'est décrite qu'à un endroit, et l'aide (`chem <commande> --help`) reste celle de la bibliothèque, en français. Les deux analyseurs acceptent la même syntaxe (`--option valeur`, `--option=valeur`, valeurs commençant par un tiret, dernière occurrence retenue, `-h` partout) et produisent les mêmes `Arguments` ; les erreurs courantes sont traduites avec les messages de la bibliothèque. Seul ajout : `--` termine les options.
- **Serde.** La structure des événements reste définie une seule fois (`Evenement::vers_json` et `depuis_json`) ; Serde ne remplace que le codage texte des valeurs JSON. Un `Formatter` dédié écrit les réels au format `Display` de Rust et les caractères de contrôle en `\u00XX` : les lignes sont identiques octet pour octet à celles de la bibliothèque, et les deux lisent les mêmes traces (entiers exacts, lecture exacte des réels avec `float_roundtrip`).
- **Miette.** Chaque `Diagnostic` est présenté à Miette avec son code stable, sa gravité, son message, ses étiquettes (la première est l'étiquette principale) et son aide. Les couleurs ne sont actives que sur un terminal sans `NO_COLOR` ; si Miette ne peut pas produire le rendu, le rendu texte de la bibliothèque prend le relais.

Utilisation :

```sh
cargo run --release --manifest-path integrations/Cargo.toml -- run exemples/b06_dissolution.chem --workers 4
cargo install --path integrations                            # commande chem avec les quatre bibliothèques
cargo build --manifest-path integrations/Cargo.toml --no-default-features --features rayon,miette
chem --version                                               # liste l'implémentation de chaque point
```

Brancher une autre bibliothèque sur un point existant : un module dans `integrations/src` qui implémente l'interface, une fonctionnalité Cargo qui active sa dépendance, une fonction de choix dans `composants()` (`#[cfg(feature = ...)]` et son inverse) et un test d'équivalence. Créer un nouveau point : une interface `Send + Sync` utilisable comme objet dans le module du moteur concerné, l'implémentation standard à côté, un champ dans `cli::Composants` initialisé par `Composants::standard()`.

Bibliothèques encore remplacées par du code maison, sans point d'intégration (elles ne servent qu'au développement) :

| Bibliothèque | Remplacée par | Pour l'intégrer |
|---|---|---|
| Thiserror | implémentations manuelles de `Display` et `Error` | `#[derive(Error)]` sur `Diagnostic`, `ErreurEvaluation`, `ErreurRejeu` |
| Proptest | `tests/commun::pour_tout` (générateur déterministe, graine du cas en échec) | `proptest!` avec les mêmes générateurs ; gain : réduction automatique des contre-exemples |
| Criterion | `benches/moteur.rs` (harnais maison) | `criterion_group!` avec un `bench_function` par scénario |

## 6. Stratégie de test

| Niveau | Où | Contenu |
|---|---|---|
| Unitaires | `#[cfg(test)]` dans chaque module | lexer, parser (erreurs localisées, profondeur), afficheur, sémantique (chaque code), évaluateur, solutions, boîte aux lettres (protocole, concurrence), réacteur (courrier, usage unique partagé, boîtes fermées), pool, garde-fous, JSON, événements, modèle de trace |
| Programmes de référence | `tests/programmes_reference.rs` | tous les exemples × stratégies × graines × workers ; recette des trois jalons ; même graine = même exécution ; format de l'annexe C |
| Programmes invalides | `tests/programmes_invalides.rs` | 23 programmes, code et position attendus ; 5 000 entrées aléatoires sans plantage |
| CLI | `tests/cli.rs` | sous-commandes, codes de retour 0 à 3, trace, rejeu, trace falsifiée, pas-à-pas |
| Concurrence | `tests/concurrence.rs` | 100 exécutions répétées (max/min, dissolution, injection synchrone et asynchrone, injections bornées) avec délai anti-interblocage ; traces parallèles re-exécutées ; injections concurrentes ; garde-fous en parallèle |
| Contraction des chaînes | `tests/contraction.rs`, `ordonnanceur::repartition` | mêmes résultats et traces vérifiées avec et sans répartition (une longue chaîne, vingt chaînes, jointure de formes distinctes, chaîne et seconde règle) ; cycles et branches sans perte ; segments comptés ; rôles des molécules, maillons consécutifs dans le même lot sauf aux coupures, chaque maillon dans un seul segment ; successeurs et segments calculés en parallèle identiques au calcul séquentiel de référence (chaînes, branches, cycles, de 1 à 8 tâches et de 1 à 4 workers) ; chaîne de 140 000 maillons découpée par les workers |
| Répartition par clé | `tests/partition.rs`, `ordonnanceur::repartition`, `moteur::jointure` | mêmes résultats et traces vérifiées avec et sans répartition (petits et grands groupes, deux règles, produits de clé nouvelle, sous-solutions, courrier reçu pendant la phase) ; fusions évitées seulement pour des lots indépendants ; détection des jointures symétriques et locales ; chaque clé dans un seul lot, charge équilibrée |
| Index par valeur | `tests/indexation.rs` | mêmes résultats et mêmes erreurs avec et sans index (règles indexables ou non, clé de l'ancre qui échoue), traces vérifiées, grande jointure, produits indexés dans la même passe |
| Points d'intégration | `tests/composants.rs` | la ligne de commande utilise les `Composants` fournis : analyse des arguments, rendu des diagnostics, format de trace (écriture et rejeu), pool de workers ; `--version` les liste |
| Paquet `integrations` | `integrations/src`, `integrations/tests/equivalence.rs` | contrat de `Parallelisme` avec Rayon ; mêmes arguments et mêmes erreurs avec Clap ; mêmes lignes de trace et même rejeu avec Serde ; diagnostics Miette complets (code, message, étiquettes, aide) ; exécutable de bout en bout ; chaque fonctionnalité seule (intégration continue) |
| Usage unique et asynchrone | `tests/usage_unique_et_asynchrone.rs` | une seule réaction par activation même répartie en lots ; règles sans réactif ; références distinctes ; course entre envois et dissolutions (conservation de la somme) ; arrêt sans perte du courrier ; réception pendant l'exécution de la cible ; traces falsifiées refusées |
| Propriétés | `tests/proprietes.rs` | oracles (max, min, somme, PGCD, crible, tri) ; invariants de programmes non confluents ; aller-retour de l'afficheur sur programmes générés ; rejeu |
| Visualiseur | `visualizer/tests` (Node 24) | analyse de trace, historique (avance, recul, accès direct), reconstruction des solutions finales du moteur, lecteur à horloge factice, disposition |
