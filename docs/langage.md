# Manuel du langage chimique

Le langage décrit des **solutions** (multi-ensembles de molécules) et des **règles de réaction** inspirées du formalisme Gamma. Une exécution applique les règles tant que c'est possible ; elle s'arrête quand la solution est **inerte**.

```
let max = replace x, y by x if x >= y ;
{ 4, 17, 9, 17, 2 | max }                 # résultat : { 17 | max }
```

## 1. Lexique

| Élément | Forme |
|---|---|
| Commentaire | `#` jusqu'à la fin de la ligne |
| Entier | suite de chiffres (`0`…`9223372036854775807`) ; le signe `-` est un opérateur |
| Chaîne | `"…"` sur une ligne ; échappements `\"` `\\` `\n` `\t` `\r` |
| Identificateur | lettre ASCII ou `_`, suivie de lettres, chiffres ou `_` |
| Mots réservés | `let replace replace-one by if and or not mod true false dissolve inject` |
| Symboles | `{ } ( ) , ; : = \| .. + - * / == != < <= > >=` |

`replace-one` s'écrit d'un seul tenant (sans espace autour du tiret) : c'est un seul mot réservé. Les noms de types (`int`, `bool`, `str`, `sol`, `ref`) ne sont pas réservés : ils ne sont reconnus qu'après `:` dans un motif. Les caractères non ASCII ne sont admis que dans les commentaires et les chaînes.

## 2. Grammaire (EBNF)

```ebnf
programme     = { definition } , solution ;
definition    = "let" , ident , "=" , regle , ";" ;
regle         = ( "replace" | "replace-one" ) , [ motif , { "," , motif } ] ,
                "by" , [ produit , { "," , produit } ] ,
                [ "if" , expr ] ;
motif         = ident , [ ":" , type ]
              | "(" , motif , "," , motif , { "," , motif } , ")" ;   (* n-uplet *)
type          = "int" | "bool" | "str" | "sol" | "ref" ;
produit       = "dissolve" , "(" , ident , ")"
              | "inject" , "(" , expr , "," , ident , ")"
              | expr ;
solution      = "{" , [ element , { "," , element } ] ,
                [ "|" , ident , { "," , ident } ] , "}" ;
element       = solution | expr , [ ".." , expr ] ;

expr          = ou ;
ou            = et , { "or" , et } ;
et            = non , { "and" , non } ;
non           = "not" , non | comparaison ;
comparaison   = additif , [ op_comp , additif ] ;        (* non associatif *)
op_comp       = "==" | "!=" | "<" | "<=" | ">" | ">=" ;
additif       = multiplicatif , { ( "+" | "-" ) , multiplicatif } ;
multiplicatif = unaire , { ( "*" | "/" | "mod" ) , unaire } ;
unaire        = "-" , unaire | primaire ;
primaire      = entier | chaine | "true" | "false" | ident
              | "(" , expr , ")"
              | "(" , expr , "," , expr , { "," , expr } , ")" ;  (* n-uplet *)
```

La grammaire est LL(1) et analysée par descente récursive. La profondeur d'imbrication est bornée (128 niveaux de parenthèses ou de solutions, 512 niveaux d'arbre d'expression) : aucune entrée ne peut épuiser la pile.

### Priorités (de la plus faible à la plus forte)

| Niveau | Opérateurs | Associativité |
|---|---|---|
| 1 | `or` | gauche |
| 2 | `and` | gauche |
| 3 | `not` | préfixe |
| 4 | `== != < <= > >=` | **aucune** : `a < b < c` est refusé (E0204) |
| 5 | `+ -` | gauche |
| 6 | `* / mod` | gauche |
| 7 | `-` unaire | préfixe |
| 8 | littéraux, variables, parenthèses, n-uplets | — |

Ainsi `not x == y` signifie `not (x == y)` et `-x * y` signifie `(-x) * y`. La commande `chem fmt` réimprime un programme avec le minimum de parenthèses compatible avec ce tableau.

## 3. Valeurs et types

| Type | Littéraux | Remarques |
|---|---|---|
| `int` | `42`, `-7` | entiers 64 bits, dépassement = erreur d'exécution |
| `bool` | `true`, `false` | |
| `str` | `"rouge"` | ordre lexicographique (points de code) |
| n-uplet | `(1, "a", true)` | au moins deux composantes, imbricables |
| `sol` | `{ … }` | une sous-solution n'est pas une valeur : elle ne se manipule que par les motifs `s:sol` (liée et consommée) et `s:ref` (référencée) |
| `ref` | — | référence à une sous-solution directe, inerte ou en cours d'exécution, qui n'est **pas consommée** : elle ne sert que de cible à `inject(e, s)` (injection asynchrone, §4.6) |

Un motif sans type (`x`) accepte **toute valeur mais jamais une sous-solution**. `x:int`, `x:bool`, `x:str` restreignent le type. Un motif n-uplet `(i, a)` n'accepte qu'un n-uplet de même arité et lie chaque composante.

## 4. Sémantique d'exécution

### 4.1 Réaction

Une **réaction** choisit une règle de la solution et des **occurrences distinctes** d'éléments, une par motif, telles que :

1. chaque élément est accepté par son motif (type, arité des n-uplets) ;
2. la condition, évaluée avec les liaisons, vaut `true` (absente = vraie).

Elle **retire** alors les réactifs et **ajoute** les produits, qui sont de nouvelles molécules (nouveaux identifiants). Deux molécules de même valeur sont distinctes : `{ 4, 4 }` contient deux réactifs possibles pour le crible.

### 4.2 Inertie et terminaison

Une solution est **inerte** quand aucune règle n'est applicable à ses éléments **et** que toutes ses sous-solutions sont inertes. L'exécution s'arrête quand la solution principale est inerte : la détection est exacte (une passe exhaustive sans réaction la prouve), quelle que soit la stratégie.

### 4.3 Non-déterminisme maîtrisé

Le langage ne fixe pas l'ordre des réactions. L'interpréteur propose :

- `--strategie aleatoire` (défaut) : ordre pseudo-aléatoire rejouable, fixé par `--seed` ;
- `--strategie ordonnee` : ordre du source, déterministe (tests).

Un programme **confluent** (maximum, somme, crible, PGCD) donne le même résultat pour toute graine ; un programme non confluent (B13) peut donner des résultats différents, tous corrects.

### 4.4 Solutions imbriquées

Une solution peut contenir des sous-solutions, chacune avec ses molécules et ses règles, à profondeur quelconque. Les sous-solutions réagissent indépendamment (en parallèle avec plusieurs workers). Une règle de la solution parente ne peut **lier une sous-solution que si elle est inerte** (motif `s:sol`). Selon la manière dont `s` apparaît dans les produits :

| Produit | Effet sur la sous-solution liée `s` |
|---|---|
| `dissolve(s)` | **dissolution** : `s` est détruite (ses règles disparaissent), ses molécules et ses sous-solutions rejoignent la solution courante |
| `inject(e, s)` | **injection** : la valeur de `e` est ajoutée dans `s`, qui garde son identité et ses règles et redevient active |
| `s` | **conservation** : `s` est remise telle quelle |
| absent | **destruction** : `s` disparaît avec tout son contenu |

Une sous-solution liée ne peut apparaître qu'une fois dans les produits (E0309) : elle n'est jamais dupliquée. La valeur injectée est une valeur (pas une sous-solution).

### 4.5 Règles à usage unique et règles sans réactif

Une règle déclarée avec **`replace-one`** est **à usage unique** : chaque activation (chaque nom dans `| …` d'une solution) ne réagit **qu'une fois**, puis disparaît de la solution. Activée deux fois (`| r, r`), elle réagit deux fois ; activée dans deux solutions, une fois dans chacune. Même avec plusieurs workers, une activation ne réagit jamais deux fois.

```
let ajouter_3 = replace-one x by x + 3 ;     # avec 'replace', s'appliquerait sans fin
{ 5 | ajouter_3, ajouter_3 }                 # résultat : { 11 } (la règle a disparu)
```

Une règle peut n'avoir **aucun réactif** : `replace by 10, 20`. Elle est applicable dans toute solution, même vide. Une telle règle — ou une règle dont tous les réactifs sont des références `s:ref`, qui ne sont pas consommées — est toujours applicable : à usage multiple, elle s'appliquerait sans fin (avertissement W0303, seul un garde-fou arrête l'exécution). Combinée à `replace-one`, elle sert à **amorcer** une solution ou à **borner des injections** :

```
let amorcer = replace-one by 10, 20, 12 ;
let somme   = replace x, y by x + y ;
{ | amorcer, somme }                         # résultat : { 42 | somme }
```

### 4.6 Injection asynchrone

Un motif **`s:ref`** désigne une sous-solution directe de la solution courante **sans la lier** : elle peut être inerte ou **en cours d'exécution**, elle n'est pas consommée et elle reste en place. `inject(e, s)` avec `s:ref` est une **injection asynchrone** : la valeur de `e` est déposée dans la **boîte aux lettres** de `s` sans attendre son inertie ; `s` la reçoit au début de sa prochaine passe de recherche et la fait réagir avec ses autres molécules.

```
let crible      = replace x, y by x if y mod x == 0 ;
let transmettre = replace x:int, s:ref by inject(x, s) ;
{ 51..100, { 2..50 | crible } | transmettre }    # le crible filtre les nombres au fil de l'eau
```

Règles :

- les références d'une réaction désignent des sous-solutions **distinctes**, distinctes aussi des sous-solutions liées par des motifs `s:sol` de la même réaction ;
- une référence peut recevoir **plusieurs** injections dans la même réaction (`inject(1, s), inject(2, s)`) ; elle ne peut ni être dissoute (E0308), ni être utilisée dans une expression ou produite seule (E0307), ni figurer dans un n-uplet (E0310) ;
- une sous-solution qui attend du courrier **n'est pas inerte** : elle ne peut pas être liée par `s:sol` tant qu'elle ne l'a pas reçu et traité. Une sous-solution liée (puis dissoute ou détruite) ne reçoit plus rien ;
- l'inertie reste exacte : une solution n'est inerte qu'une fois tout son courrier reçu et traité.

En mode séquentiel (1 worker), la sous-solution reçoit le courrier à son tour suivant ; avec plusieurs workers, elle le reçoit pendant qu'elle réagit. Pour un programme confluent, le résultat ne dépend pas du moment de la réception.

| Produit, avec `s` lié par… | `s:sol` (sous-solution inerte, consommée) | `s:ref` (référence, non consommée) |
|---|---|---|
| `inject(e, s)` | injection **synchrone** : `s` est réactivée | injection **asynchrone** : `e` part en transit vers `s` |
| `dissolve(s)` | dissolution | interdit (E0308) |
| `s` | conservation | interdit (E0307) |
| absent | destruction | rien : `s` reste en place |

### 4.7 Expressions

- `+` additionne deux entiers ou concatène deux chaînes ; `- *` sur les entiers ;
- `/` et `mod` sont **euclidiens** : `-7 / 3 = -3`, `-7 mod 3 = 2` (le reste est toujours positif ou nul) ;
- `==` et `!=` comparent deux valeurs quelconques (types différents : simplement différentes) ;
- `< <= > >=` comparent deux entiers ou deux chaînes ;
- `and` et `or` sont paresseux (`false and (1/0 == 0)` vaut `false`) ;
- l'évaluation est réalisée par un interpréteur interne ; aucune évaluation dynamique du langage hôte n'est utilisée.

### 4.8 Solution initiale

Les éléments d'une solution écrite dans le source sont des **constantes** (pas de variables, E0311), évaluées au chargement. `a..b` produit les entiers de `a` à `b` inclus (E0314 si `a > b`). La taille initiale est bornée par `--max-molecules`.

## 5. Vérifications avant exécution

`chem check` (et `chem run` avant d'exécuter) signale toutes les erreurs détectables statiquement, avec fichier, ligne, colonne et extrait souligné :

```
erreur[E0304]: variable 'z' non liee
 --> b1.chem:2:37
  |
2 | let max = replace x, y by x if x >= z ;
  |                                     ^ absente des reactifs
  |
  = aide : vouliez-vous dire 'x' ?
```

Les variables non annotées étant de type « valeur », seules les incohérences certaines sont refusées ; les autres sont vérifiées à l'exécution (E0403).

## 6. Erreurs d'exécution

Une erreur pendant l'évaluation d'une condition ou d'un produit (division par zéro, dépassement, type) **arrête l'exécution** : le diagnostic cite la règle, l'opération fautive et les réactifs, la solution au moment de l'arrêt est affichée et le code de retour vaut 2. Aucun réactif n'est consommé par la réaction fautive.

## 7. Codes des diagnostics

| Code | Signification |
|---|---|
| E0101 | caractère inattendu |
| E0102 | chaîne non terminée |
| E0103 | entier trop grand |
| E0104 | séquence d'échappement invalide |
| E0105 | `.` isolé (les intervalles s'écrivent `a..b`) |
| E0106 | `!` isolé (la différence s'écrit `!=`) |
| E0201 | jeton attendu (`;`, `by`, `)`, expression…) |
| E0202 | solution principale manquante |
| E0203 | texte superflu après la solution principale |
| E0204 | comparaisons enchaînées sans parenthèses |
| E0205 | type inconnu dans un motif |
| E0206 | `dissolve` ou `inject` hors d'un produit |
| E0207 | imbrication trop profonde |
| E0301 | règle définie plusieurs fois |
| E0302 | règle activée mais non définie (avec suggestion) |
| E0303 | variable liée plusieurs fois dans les réactifs |
| E0304 | variable utilisée mais non liée |
| E0305 | condition non booléenne |
| E0306 | opération entre types incompatibles |
| E0307 | sous-solution ou référence (`ref`) utilisée dans une expression ou produite seule |
| E0308 | `dissolve`/`inject` sur un réactif qui n'est ni `sol` ni `ref` ; `dissolve` sur une référence |
| E0309 | sous-solution utilisée plusieurs fois dans les produits |
| E0310 | motif `sol` ou `ref` dans un n-uplet |
| E0311 | variable dans une solution littérale |
| E0312 | borne d'intervalle non entière |
| E0313 | constante non évaluable (ex. `1 / 0`) |
| E0314 | intervalle vide |
| E0315 | solution initiale trop grande |
| E0399 | erreur interne de compilation |
| E0401 | division par zéro (exécution) |
| E0402 | dépassement de capacité (exécution) |
| E0403 | opération sur des valeurs du mauvais type (exécution) |
| W0301 | règle à usage multiple activée deux fois dans la même solution (pour `replace-one`, deux activations valent deux usages : pas d'avertissement) |
| W0302 | règle définie mais jamais activée |
| W0303 | règle à usage multiple qui ne consomme aucun réactif : toujours applicable, elle s'appliquerait sans fin (utiliser `replace-one`) |

## 8. Programmes de référence

Les programmes de l'annexe B du cahier des charges et d'autres exemples se trouvent dans `exemples/` ; chacun indique son résultat attendu (`# attendu : …`), vérifié automatiquement par `engine/tests/programmes_reference.rs`.

| Fichier | Contenu | Résultat |
|---|---|---|
| `b01_maximum.chem` | maximum (`x >= y`) | `{ 17 \| max }` |
| `b02_somme.chem` | somme de 1 à 100 | `{ 5050 \| somme }` |
| `b03_crible.chem` | crible jusqu'à 50 | les 15 premiers nombres premiers |
| `b04_pgcd.chem` | PGCD de 12, 18, 30 | `{ 6 \| … }` |
| `b05_tri.chem` | tri par échanges sur n-uplets | `{ (0, 1), (1, 3), (2, 5), (3, 8) \| echanger }` |
| `b06_dissolution.chem` | maximums en parallèle puis dissolution | `{ 21 \| vider, max }` |
| `b07_injection.chem` | injection et réactivation | `{ { 25 \| max } \| envoyer }` |
| `b08_max_min.chem` | maximum et minimum en parallèle | `{ { 17 \| max }, { 2 \| min } }` |
| `b09_crible_100.chem` | crible jusqu'à 100 (démonstration) | 25 nombres premiers |
| `b10_max_doublons.chem` | maximum avec `x > y` | `{ 17, 17 \| max_strict }` |
| `b11_factorielle.chem` | produit de 1 à 10 | `{ 3628800 \| produit }` |
| `b12_decompte.chem` | règle d'arité 1 | `{ 0, 0, 0, 0 \| decompter }` |
| `b13_difference.chem` | **non confluent** (invariant de parité) | une valeur impaire |
| `b14_arite3.chem` | règle d'arité 3 | `{ 45 \| somme3 }` |
| `b15_imbrication.chem` | trois niveaux de dissolution | `{ 10 \| vider, somme }` |
| `b16_chaines.chem` | chaînes et booléens | `{ false, "rouge" \| … }` |
| `b17_usage_unique.chem` | règles à usage unique (`replace-one`), activées une ou deux fois | `{ 15 }` |
| `b18_sans_reactif.chem` | règle sans réactif dans une solution vide | `{ 42 \| somme }` |
| `b19_injection_asynchrone.chem` | crible alimenté pendant son exécution (`s:ref`) | 25 nombres premiers dans la sous-solution |
| `b20_injection_bornee.chem` | injections bornées (`replace-one` + `s:ref`) | `{ { 50 \| max } }` |
