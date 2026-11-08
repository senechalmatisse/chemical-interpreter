# Rapport de performance

## Méthodologie

- **Banc** : `cargo bench` (harnais `engine/benches/moteur.rs`, profil optimisé) ou `scripts/chemxpe01.sh`.
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

### Surcoût de la trace JSON Lines (EF-V7)

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

**Limites du langage hôte.** Rust n'a ni verrou global ni ramasse-miettes : le parallélisme est réel et les pauses inexistantes. Les limites viennent de la mémoire (allocation et parcours des grands vecteurs) et de la création de threads (quelques dizaines de µs par `joindre`, amorties par des lots d'au moins 64 éléments). Un pool à vol de tâches (Rayon, voir `docs/architecture.md`) réduirait ce dernier coût.

## Impact des règles à usage unique et de l'injection asynchron

Ces fonctionnalités ne coûtent rien aux programmes qui ne les utilisent pas : la boucle de recherche ne fait que deux lectures supplémentaires par couple (ancre, règle) - la règle est-elle déjà retirée, y a-t-il assez de sous-solutions pour ses références ? -, le relevé du courrier en début de passe lit un indicateur atomique sans prendre de verrou, et le protocole de boîte aux lettres (fermeture des sous-solutions liées) n'est actif que dans une solution qui a au moins une règle avec une référence `s:ref`. Le banc complet a été relancé après leur ajout : toutes les mesures restent dans la variabilité de la machine virtuelle (par exemple `crible_1000` : 19,7 ms ; `decompte_3000` à 2 workers : accélération 1,90 ; `sous_solutions_8` à 2 workers : 1,87).

Une injection asynchrone coûte deux sections critiques très courtes (réservation, dépôt) et, quand la trace est active, la diffusion immédiate du lot d'événements de l'expéditeur, qui garantit l'ordre causal envoi → réception dans la trace.

## Pistes d'amélioration

- indexation des molécules par valeur pour les conditions d'égalité (`x == y`) ;
- découpage du crible par plages de valeurs plutôt que par répartition circulaire ;
- molécules entières stockées sans enveloppe (`Vec<i64>` spécialisé) pour réduire la mémoire de moitié ;
- Rayon pour le vol de tâches et la réutilisation des threads.
