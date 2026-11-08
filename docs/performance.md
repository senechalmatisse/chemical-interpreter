# Rapport de performance

## Méthodologie

- **Banc** : `cargo bench` (harnais `engine/benches/moteur.rs`, profil optimisé) ou `scripts/chemxpe01.sh` ; pool Rayon face au pool standard : `scripts/chemxpe02.sh` (harnais `integrations/benches/pools.rs`, mêmes programmes, définis dans `engine/benches/commun/scenarios.rs`).
- **Mesure** : durée de l'exécution par l'ordonnanceur (`Statistiques::duree`), hors lecture, analyse et compilation du programme. Une exécution d'échauffement puis 5 mesures ; on retient la **médiane** (et le minimum).
- **Indicateurs** : accélération `S(n) = T(1) / T(n)`, efficacité `E(n) = S(n) / n`.
- **Configuration** : stratégie aléatoire, graine 2026, trace désactivée sauf mention contraire.
- **Machine de mesure** : machine virtuelle à **2 cœurs** (Intel Xeon 2,1 GHz), 8 Go, Linux, rustc 1.97 en mode `release`. Avec 2 cœurs, l'accélération est plafonnée à 2 : les mesures à 4 workers montrent le comportement en sur-souscription. À reproduire sur une machine à 4 cœurs ou plus pour la soutenance.

## Scénarios

| Scénario | Programme | Réactions | Nature |
|---|---|---:|---|
| `maximum_10000` | maximum de 10 000 entiers aléatoires | 9 999 | réactions très fines, toujours applicables |
| `maximum_100000` | maximum de `1..100000` | 99 999 | idem, plus grand |
| `crible_1000` | crible de 2 à 1 000 | 831 | recherche coûteuse (beaucoup de couples refusés) |
| `crible_5000` | crible de 2 à 5 000 | 4 330 | idem, quadratique |
| `somme_1000000` | somme de `1..1000000` | 999 999 | réactions fines, limité par la mémoire |
| `decompte_3000` | `replace x by x - 1 if x > 0` sur `1..3000` | 4 501 500 | **réactions indépendantes** (arité 1) |
| `sous_solutions_8` | 8 cribles de 2 à 1 500 dans des sous-solutions | 8 × 1 260 | parallélisme entre sous-solutions (EF-S2) |
| `jointure_chaine_3000` | 3 000 maillons `(i, i + 1)` mélangés, refermés par `if b == c` | 2 999 | jointure sur égalité (index par valeur) |
| `jointure_chaine_30000` | idem, 30 000 maillons | 29 999 | contraction parallèle des chaînes |
| `regroupement_20000` | somme par clé de 20 000 couples `(clé, valeur)`, 50 clés, `if k == l` | 19 950 | égalité à grands seaux (index par valeur) |
| `doublons_40000` | 40 000 valeurs parmi 10 000, `replace x, y by x if x == y` | ≈ 30 000 | jointure symétrique à petits groupes (répartition par clé) |
| `regroupement_40000` | somme par clé de 40 000 couples, 10 000 clés | ≈ 30 000 | idem, couples |

## Résultats

| scénario | workers | médiane (ms) | min (ms) | accélération | efficacité |
|---|---:|---:|---:|---:|---:|
| maximum_10000 | 1 | 1,27 | 1,26 | 1,00 | 1,00 |
| maximum_10000 | 2 | 1,06 | 0,90 | 1,20 | 0,60 |
| maximum_10000 | 4 | 1,26 | 1,15 | 1,01 | 0,25 |
| crible_1000 | 1 | 22,91 | 22,77 | 1,00 | 1,00 |
| crible_1000 | 2 | 19,07 | 18,65 | 1,20 | 0,60 |
| crible_1000 | 4 | 22,01 | 20,58 | 1,04 | 0,26 |
| crible_5000 | 1 | 455,56 | 443,06 | 1,00 | 1,00 |
| crible_5000 | 2 | 380,32 | 355,00 | 1,20 | 0,60 |
| crible_5000 | 4 | 402,47 | 394,06 | 1,13 | 0,28 |
| decompte_3000 | 1 | 632,78 | 597,19 | 1,00 | 1,00 |
| decompte_3000 | 2 | 308,79 | 297,13 | **2,05** | 1,02 |
| decompte_3000 | 4 | 308,41 | 299,49 | **2,05** | 0,51 |
| maximum_100000 | 1 | 12,89 | 12,86 | 1,00 | 1,00 |
| maximum_100000 | 2 | 8,26 | 8,19 | 1,56 | 0,78 |
| maximum_100000 | 4 | 10,54 | 8,49 | 1,22 | 0,31 |
| somme_1000000 | 1 | 135,25 | 132,23 | 1,00 | 1,00 |
| somme_1000000 | 2 | 89,09 | 85,20 | 1,52 | 0,76 |
| somme_1000000 | 4 | 100,94 | 92,05 | 1,34 | 0,33 |
| sous_solutions_8 | 1 | 383,91 | 377,26 | 1,00 | 1,00 |
| sous_solutions_8 | 2 | 194,23 | 189,76 | **1,98** | 0,99 |
| sous_solutions_8 | 4 | 202,83 | 196,87 | **1,89** | 0,47 |
| jointure_chaine_3000 | 1 | 1,31 | 1,29 | 1,00 | 1,00 |
| jointure_chaine_3000 | 2 | 1,82 | 1,62 | 0,72 | 0,36 |
| jointure_chaine_3000 | 4 | 2,29 | 2,26 | 0,57 | 0,14 |
| regroupement_20000 | 1 | 9,54 | 9,36 | 1,00 | 1,00 |
| regroupement_20000 | 2 | 8,56 | 8,03 | 1,11 | 0,56 |
| regroupement_20000 | 4 | 10,07 | 9,21 | 0,95 | 0,24 |
| doublons_40000 | 1 | 22,44 | 20,16 | 1,00 | 1,00 |
| doublons_40000 | 2 | 14,52 | 13,90 | **1,55** | 0,77 |
| doublons_40000 | 4 | 12,81 | 10,83 | 1,75 | 0,44 |
| regroupement_40000 | 1 | 43,62 | 35,00 | 1,00 | 1,00 |
| regroupement_40000 | 2 | 33,74 | 25,77 | 1,29 | 0,65 |
| regroupement_40000 | 4 | 26,82 | 25,19 | 1,63 | 0,41 |

Les quatre dernières jointures ont été mesurées après l'ajout de la répartition des lots par clé (section suivante) ; la machine virtuelle varie de ±20 % d'une série à l'autre, d'où l'écart avec les valeurs de la section sur l'index.

### Surcoût de la trace JSON Lines

| scénario | sans trace (ms) | avec trace (ms) | surcoût |
|---|---:|---:|---:|
| maximum_10000 | 1,27 | 13,80 | +985 % |
| crible_1000 | 22,66 | 24,21 | +7 % |
| maximum_100000 | 13,29 | 145,72 | +997 % |

## Objectifs du cahier des charges

| Objectif (indicatif) | Mesure | Statut |
|---|---|---|
| maximum de 10 000 entiers en moins de 5 s | 1,3 ms | atteint (×3 900) |
| crible jusqu'à 1 000 en moins de 10 s (séquentiel) | 23 ms | atteint (×430) |
| accélération ≥ 1,5 avec 4 workers sur un programme à réactions indépendantes | 2,05 (`decompte_3000`) ; 1,89 (`sous_solutions_8`) | atteint, sur 2 cœurs seulement |

## Analyse

**Granularité.** Une réaction de `maximum` ou de `somme` coûte environ 100 ns. À ce grain, ce qui entoure la réaction compte autant qu'elle : déplacement des éléments vers les lots, fusions, compactage. L'accélération des petites instances (`maximum_10000` : 1,3 ms au total) reste donc faible, et celle des grandes (`maximum_100000`, `somme_1000000` : 1,5) est bornée par la **bande passante mémoire** (un million de molécules de 40 octets, parcourues plusieurs fois). Le `decompte`, dont chaque molécule réagit des milliers de fois sur place, atteint l'accélération maximale de la machine (2,05 sur 2 cœurs, légèrement super-linéaire grâce à de meilleurs effets de cache sur des lots plus petits).

**Contention.** Elle a été éliminée par construction : aucun verrou n'est pris pendant la recherche ; les jetons d'étape, les identifiants de molécules et les compteurs sont prélevés par blocs (256 jetons, 1 024 identifiants) et fusionnés en fin de lot ; les événements de trace sont regroupés par lots de 512. La seule donnée partagée à chaque réaction est un drapeau d'arrêt en lecture.

**Partitionnement et algorithme.** Le parallélisme interne à une solution change le travail effectué. Pour les réductions (maximum, somme), chaque lot se réduit à une molécule et la fusion est triviale. Pour le crible, chaque lot élimine les multiples dont un diviseur est dans le même lot ; les survivants (premiers et composés « orphelins ») sont rassemblés et la passe exhaustive finale, quadratique, reste séquentielle : l'accélération plafonne à 1,2. Le parallélisme entre sous-solutions (`sous_solutions_8`), sans partage, est quasi idéal (1,98 sur 2 cœurs).

**Répartition du budget de workers.** Les workers sont répartis entre une solution et ses sous-solutions actives selon la structure. Une première version partitionnait chaque sous-solution avec tous les workers alors que les places étaient déjà occupées par les sous-solutions voisines : les lots s'exécutaient alors l'un après l'autre et le partitionnement n'apportait que son surcoût (1,25 au lieu de 1,89 pour `sous_solutions_8` à 4 workers). Le budget a corrigé ce cas tout en rendant le découpage déterministe.

**Sur-souscription.** Avec 4 workers sur 2 cœurs, les threads se partagent les cœurs : l'efficacité est divisée par deux et le surcoût des lots supplémentaires apparaît (crible). Les mesures à 4 workers sont à refaire sur 4 cœurs.

**Trace.** Désactivée, la trace ne coûte qu'un test par réaction (diffuseur sans abonné : les événements ne sont pas construits). Activée, elle coûte environ 1,3 µs par événement (construction, sérialisation JSON, écriture tamponnée) : négligeable quand la recherche domine (crible : +7 %), dominante pour les réactions de 100 ns (maximum : ×11). Pour les très longues exécutions, il vaut mieux tracer une instance réduite.

**Limites du langage hôte.** Rust n'a ni verrou global ni ramasse-miettes : le parallélisme est réel et les pauses inexistantes. Les limites viennent de la mémoire (allocation et parcours des grands vecteurs) et de la création de threads (quelques dizaines de µs par `joindre`, amorties par des lots d'au moins 64 éléments). Le pool Rayon du paquet `integrations` (voir `docs/architecture.md`, section 5) supprime ce dernier coût : ses threads sont créés une fois par exécution et le vol de tâches équilibre la charge. Il n'a pas encore été mesuré sur cette machine (dépendances indisponibles hors ligne) : `cargo run --release --manifest-path integrations/Cargo.toml -- run <programme> --workers 4 --stats` donne la durée à comparer avec celle du moteur seul.

## Indexation par valeur des conditions d'égalité

Une règle à deux réactifs dont la condition contient une égalité entre les deux réactifs (`x == y`, `b == c`, `k == l`) ne cherche plus ses partenaires parmi tout le lot, mais dans le seau de l'index qui a la même clé (`docs/architecture.md`, section 3.5). Mesures à 1 worker (`cargo bench --bench moteur -- index`), même machine :

| scénario | sans index (ms) | avec index (ms) | gain |
|---|---:|---:|---:|
| `doublons_4000` (4 000 valeurs parmi 3 000, `replace x, y by x if x == y`) | 788,28 | 1,49 | **×528** |
| `jointure_chaine_3000` | 165,41 | 1,63 | **×101** |
| `regroupement_20000` | 122,25 | 11,14 | **×11** |

Le gain dépend de la taille des seaux. Avec des clés presque toutes distinctes (doublons, chaîne), chaque recherche ne regarde qu'un ou deux candidats au lieu de tout le lot : la passe devient linéaire. Avec 50 clés pour 20 000 couples, chaque seau contient environ 400 couples et le gain se limite au rapport entre le lot et le seau. Les règles sans égalité indexable ne sont pas touchées : l'analyse est faite à la compilation et le réacteur n'a pas d'index pour elles (le crible, dont la condition `y mod x == 0` mêle les deux réactifs, garde son temps). `chem run --sans-index` permet de comparer sur n'importe quel programme (seul l'ordre d'essai des partenaires change : un programme confluent donne le même résultat).

En parallèle, la chaîne ne se referme qu'une fois ses maillons réunis dans un même lot : sur une exécution de 1,5 ms, le découpage en lots n'apporte que son surcoût (`jointure_chaine_3000` : 1,82 ms à 2 workers). La section suivante traite le cas des jointures symétriques.

## Répartition des lots guidée par les jointures

En distribution circulaire, deux molécules de même clé tombent presque toujours dans des lots différents : avec 10 000 clés et 4 lots, une molécule n'a qu'une chance sur quatre de trouver son partenaire dans son lot, et l'essentiel des réactions attend la fusion, séquentielle. Pour une jointure symétrique (même motif et même clé pour les deux réactifs, comme `x == y` ou `k == l`), l'ordonnanceur range désormais les molécules par clé : chaque groupe de même clé va en entier dans un lot, les lots font tout le travail en parallèle, et quand la règle garde la clé de ses réactifs (jointure locale) la phase se passe même des fusions (`docs/architecture.md`, section 3.6). Mesures `cargo bench --bench moteur -- partition`, même machine à 2 cœurs, circulaire et par clé mesurés dans la même série :

| scénario | workers | circulaire (ms) | par clé (ms) | gain | accélération par clé |
|---|---:|---:|---:|---:|---:|
| `doublons_40000` | 2 | 23,46 | 16,33 | **×1,44** | 1,20 |
| `doublons_40000` | 4 | 35,02 | 15,64 | **×2,24** | 1,26 |
| `regroupement_40000` | 2 | 39,62 | 25,50 | **×1,55** | 1,64 |
| `regroupement_40000` | 4 | 55,37 | 31,40 | **×1,76** | 1,33 |
| `regroupement_20000` (50 clés) | 2 | 8,44 | 8,88 | ×0,95 | 1,52 |
| `regroupement_20000` (50 clés) | 4 | 9,12 | 10,50 | ×0,87 | 1,29 |

- **Petits groupes** (en moyenne 4 molécules par clé) : en circulaire, plus il y a de lots, moins un lot trouve de couples, et le temps augmente avec les workers (`doublons_40000` : 23 puis 35 ms). Par clé, le temps diminue au contraire, et l'écart atteint ×2,2 à 4 workers.
- **Grands groupes** (50 clés de 400 couples) : chaque lot réduit déjà chacune de ses clés en circulaire ; la répartition par clé n'apporte rien et coûte le calcul des clés, environ 5 % (10 % en sur-souscription).
- **Coût de la répartition** : la clé de chaque molécule est calculée une fois par phase, en parallèle (une tranche par lot), puis les groupes sont placés du plus grand au plus petit dans le lot le moins chargé ; un groupe plus grand qu'un lot est distribué de façon circulaire, ce qui préserve la réduction en parallèle des très grands groupes.

Avec 2 cœurs, l'accélération reste bornée à 2 : ces gains sont à confirmer sur une machine à 4 cœurs ou plus, comme ceux du pool Rayon (plus bas). `chem run --sans-partition` rétablit la distribution circulaire pour comparer sur n'importe quel programme.

### Contraction parallèle des chaînes (jointures asymétriques)

Dans la chaîne `replace (a, b), (c, d) by (a, d) if b == c`, un maillon n'a pas la même clé dans ses deux rôles : aucune clé unique ne réunit les couples. L'ordonnanceur relie chaque maillon à son successeur, coupe les chaînes en segments (environ 8 par lot) et place chaque segment en entier dans un lot, qui le contracte en une molécule (`docs/architecture.md`, section 3.6). Mêmes mesures, même machine :

| scénario | workers | circulaire (ms) | par segments (ms) | gain | accélération par segments |
|---|---:|---:|---:|---:|---:|
| `jointure_chaine_3000` | 2 | 1,67 | 1,47 | ×1,13 | 0,95 |
| `jointure_chaine_3000` | 4 | 2,23 | 1,97 | ×1,13 | 0,71 |
| `jointure_chaine_30000` | 2 | 24,14 | 20,74 | ×1,16 | **1,43** |
| `jointure_chaine_30000` | 4 | 31,49 | 19,54 | **×1,61** | **1,51** |

- **Avant** : en circulaire, la chaîne avançait surtout dans le lot final (`jointure_chaine_30000` : 24 ms à 2 workers et 31 ms à 4, pour environ 30 ms à 1 worker). **Après** : chaque lot contracte ses segments, et la chaîne de 30 000 maillons gagne 1,4 à 1,5 sur 2 cœurs ; une deuxième série a donné 1,41 et 1,35 (la machine varie de ±30 % sur ces durées).
- **Petites chaînes** : à 3 000 maillons (1,4 ms en tout), le temps de la répartition et du démarrage des threads égale le gain ; le découpage reste un peu meilleur que la distribution circulaire, mais pas plus rapide qu'un seul worker.
- **Découpage** : la construction des successeurs et le parcours des segments, d'abord séquentiels (une table de hachage unique, puis un passage sur les maillons), prenaient environ 1,5 ms sur les 20 ms de la chaîne de 30 000 maillons à 2 workers. Avec plus de cœurs, cette part aurait limité l'accélération : la sous-section suivante l'accélère et la partage entre les workers.
- **Cycles et branches** : un cycle reçoit un repère, une branche (deux maillons pour une même clé d'ancre) coupe la chaîne en deux segments ; dans les deux cas le résultat reste une exécution séquentielle possible, vérifiée par le rejeu des traces dans les tests.

### Construction parallèle des successeurs

Le découpage d'une chaîne relie chaque maillon à son successeur, puis parcourt les segments d'un repère au suivant. Ces deux étapes sont désormais réparties (`docs/architecture.md`, section 3.6) :

- les clés sont réparties en plages d'empreintes, chacune avec sa propre table, plus petite et plus souvent dans le cache. Un maillon et son successeur ont une clé commune, donc la même plage, et chaque plage est traitée par une tâche ;
- chaque tâche n'écrit que dans ses propres données (les résultats de sa plage, puis sa partie du tableau des successeurs) : les cœurs ne se disputent pas les mêmes lignes de cache, et il n'y a ni verrou ni opération atomique ;
- une tâche parcourt jusqu'à 16 segments de front, d'un maillon chacun à tour de rôle : le processeur recouvre leurs lectures au lieu d'attendre la mémoire à chaque maillon ;
- le résultat est exactement celui du calcul séquentiel, quel que soit le nombre de tâches : un test compare les deux calculs sur des chaînes, des branches et des cycles, de 1 à 8 tâches et de 1 à 4 workers.

Mesures de l'étape seule (chaîne mélangée, 2 lots, médiane de 31 exécutions alternées entre les trois versions, même machine) :

| maillons | avant (ms) | une tâche (ms) | deux tâches (ms) |
|---:|---:|---:|---:|
| 3 000 | 0,08 | 0,06 | 0,25 |
| 30 000 | 1,42 | 0,99 | 1,01 |
| 100 000 | 4,51 | 2,39 | 2,64 |
| 300 000 | 31,48 | 15,32 | 12,07 |

- **Sur un seul cœur**, les tables par plage et les parcours entrelacés divisent le temps par 1,4 à 30 000 maillons et par 2 à partir de 100 000. D'une série à l'autre, les durées varient de ±30 %, mais les rapports restent stables.
- **En parallèle**, le pool standard démarre un thread par tâche : sur cette machine, deux tâches ne gagnent qu'au-delà d'environ 100 000 maillons (×1,15 à ×1,30 à 300 000). L'étape n'est donc partagée entre les workers qu'à partir de 131 072 maillons. Avec le pool Rayon, dont les threads sont déjà démarrés, ce seuil pourrait être plus bas : il reste à mesurer sur 4 cœurs ou plus.
- **Sur l'exécution complète**, l'écart reste dans le bruit de la machine (trois versions du moteur lancées en alternance) : l'étape ne pèse que quelques pour cent d'une contraction, environ 1 ms sur les 24 ms de la chaîne de 30 000 maillons à 2 workers et 12 ms sur les 450 ms de celle de 300 000. Son intérêt est de ne plus limiter l'accélération quand les cœurs sont plus nombreux.

`cargo test --release --lib mesure_des_successeurs -- --ignored --nocapture` refait la mesure d'une tâche face à une tâche par lot, pour 2 et 4 lots.

## Pool Rayon face au pool standard

Le paquet `integrations` remplace le pool de la bibliothèque standard, qui démarre un thread pour chaque branche exécutée en parallèle, par un `rayon::ThreadPool` créé pour l'exécution, dont les threads se partagent le travail par vol de tâches. Le harnais `integrations/benches/pools.rs` les compare sur les mêmes programmes que le banc du moteur, avec 1, 2 et 4 workers, puis 8 et le nombre de cœurs quand la machine en a plus. Les mesures alternent entre les deux pools pour qu'une variation de la machine les touche tous les deux, et la création des threads est comptée dans le temps de chaque exécution. Le tableau donne pour chaque scénario les deux temps, le rapport Rayon / standard (inférieur à 1 : Rayon plus rapide) et l'accélération de chaque pool par rapport à son propre temps à 1 worker ; une synthèse donne la moyenne géométrique du rapport pour chaque nombre de workers.

Ces mesures demandent au moins 4 cœurs et le téléchargement de Rayon. Elles n'ont donc pas pu être prises sur la machine virtuelle de ce rapport (2 cœurs, sans accès au réseau). Pour les obtenir :

- en local, sur une machine à 4 cœurs ou plus : `scripts/chemxpe02.sh` (résultats dans `docs/mesures-pools-AAAAMMJJ.md`) ;
- en intégration continue : le job « Pool Rayon face au pool standard » du workflow Performance tourne sur `ubuntu-latest` (4 cœurs pour un dépôt public) à chaque demande de fusion qui touche le moteur ou le pool Rayon. On peut aussi le lancer à la main (Actions > Performance > Run workflow) en choisissant la machine, par exemple `ubuntu-24.04-arm` (4 cœurs ARM). Le tableau s'affiche dans le récapitulatif du job et reste disponible 90 jours en artefact (`mesures-pools`).

Ce que les mesures permettront de trancher : à 1 worker, les deux pools exécutent tout sur un seul thread et doivent être à égalité (seul Rayon crée un thread) ; à 2 et 4 workers, Rayon devrait gagner sur les phases courtes et nombreuses (création de threads évitée) et sur les sous-solutions déséquilibrées (vol de tâches), et rester à égalité sur les longues réactions indépendantes (`decompte_3000`), où le coût des threads est négligeable.

## Impact des règles à usage unique et de l'injection asynchrone

Ces fonctionnalités ne coûtent rien aux programmes qui ne les utilisent pas : la boucle de recherche ne fait que deux lectures supplémentaires par couple (ancre, règle) - la règle est-elle déjà retirée, y a-t-il assez de sous-solutions pour ses références ? -, le relevé du courrier en début de passe lit un indicateur atomique sans prendre de verrou, et le protocole de boîte aux lettres (fermeture des sous-solutions liées) n'est actif que dans une solution qui a au moins une règle avec une référence `s:ref`. Le banc complet a été relancé après leur ajout : toutes les mesures restent dans la variabilité de la machine virtuelle (par exemple `crible_1000` : 19,7 ms ; `decompte_3000` à 2 workers : accélération 1,90 ; `sous_solutions_8` à 2 workers : 1,87).

Une injection asynchrone coûte deux sections critiques très courtes (réservation, dépôt) et, quand la trace est active, la diffusion immédiate du lot d'événements de l'expéditeur, qui garantit l'ordre causal envoi → réception dans la trace.
