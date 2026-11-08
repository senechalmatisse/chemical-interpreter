# Format de la trace

`chem run … --trace-jsonl fichier.jsonl` écrit **un événement JSON par ligne**. Le format étend l'annexe C du cahier des charges ; il est lu par `chem replay`, par la trace console et par le visualiseur web.

```json
{"t":0,"ev":"debut","version":2,"graine":"0","strategie":"ordonnee","workers":1,"programme":"exemples/b01_maximum.chem"}
{"t":0.000017,"ev":"init","sol":"s0","parent":null,"regles":["max"],"mol":[{"id":1,"v":4},{"id":2,"v":17},{"id":3,"v":9},{"id":4,"v":17},{"id":5,"v":2}]}
{"t":0.000049,"ev":"reaction","n":1,"w":0,"sol":"s0","regle":"max","in":[2,1],"out":[{"id":6,"v":17}]}
…
{"t":0.000093,"ev":"inertie","sol":"s0"}
{"t":0.000109,"ev":"fin","reactions":4,"inerte":true,"motif":"inertie"}
```

La version 2 ajoute les règles à usage unique (champ `retiree` des réactions) et l'injection asynchrone (événements `envoi` et `reception`). Une trace de version 1 est une trace de version 2 valide : les lecteurs acceptent les deux.

## Conventions

- `t` : secondes écoulées depuis le début de l'exécution (précision : la microseconde).
- Les solutions sont identifiées par `"sN"` (`s0` = solution principale), les molécules par un entier **stable** : une molécule garde son identifiant tant qu'elle existe, y compris quand elle change de solution (dissolution, réception du courrier). Les produits reçoivent de nouveaux identifiants, uniques sur toute l'exécution (ils ne sont pas forcément consécutifs).
- Valeurs `v` : entier JSON, booléen, chaîne, ou tableau pour un n-uplet (`[0, 5]` représente `(0, 5)`). Les entiers sont écrits exactement (64 bits) ; un lecteur JavaScript les arrondit au-delà de 2⁵³.
- Ordre : les événements d'une même réaction (réaction puis effets) sont contigus. Entre workers, l'ordre des lignes respecte la causalité : une molécule est toujours produite avant d'être consommée, un `envoi` précède toujours la `reception` de ses molécules, et une réaction qui lie puis conserve une sous-solution précède tout envoi ultérieur vers celle-ci. Les horodatages de workers différents peuvent ne pas être croissants : l'ordre des lignes fait foi.

## Événements

| `ev` | Champs | Sens |
|---|---|---|
| `debut` | `version`, `graine` (texte : un entier 64 bits non signé), `strategie`, `workers`, `programme` | paramètres de l'exécution (première ligne) |
| `init` | `sol`, `parent` (`null` pour la racine), `regles`, `mol` (`[{id, v}]`) | état initial d'une solution ; ordre préfixe : un parent avant ses enfants |
| `reaction` | `n` (numéro global à partir de 1), `w` (worker), `sol`, `regle`, `in` (ids des molécules consommées, dans l'ordre des motifs ; vide pour une règle sans réactif), `in_sol` (sous-solutions liées, omis si vide), `out` (molécules produites dans `sol`), `retiree` (`true` si la règle, à usage unique, est retirée de `sol` ; omis sinon) | application d'une règle |
| `dissolution` | `n`, `sol` (parent), `cible`, `mol` (ids libérés), `sous` (sous-solutions libérées) | effet de `dissolve(cible)` de la réaction `n` |
| `injection` | `n`, `sol`, `cible`, `out` (molécules ajoutées dans `cible`) | effet de `inject(…, cible)` avec `cible:sol` ; `cible` redevient active |
| `envoi` | `n`, `sol`, `cible` (sous-solution directe de `sol`), `out` (molécules envoyées) | effet de `inject(…, cible)` avec `cible:ref` : les molécules partent **en transit** vers la boîte aux lettres de `cible`, qui peut être en cours d'exécution. Un événement par référence de la règle |
| `reception` | `sol`, `mol` (ids des molécules reçues) | relevé du courrier : les molécules en transit rejoignent le multi-ensemble de `sol`, qui n'est plus inerte |
| `destruction` | `n`, `sol`, `cible` | sous-solution liée non reproduite : détruite avec son contenu |
| `inertie` | `sol` | la solution vient d'atteindre l'inertie (elle peut redevenir active après une injection ou une réception) |
| `fin` | `reactions`, `inerte`, `motif` (`inertie`, `max-etapes`, `delai`, `max-molecules`, `interruption`, `erreur`) | dernière ligne |

Une sous-solution liée par une réaction (`in_sol`) et ne faisant l'objet d'aucun effet (`dissolution`, `injection`, `destruction`) a été **conservée** telle quelle (`by s`).

Exemple (programme `b20_injection_bornee.chem`, 4 workers) : la règle à usage unique `alimenter` réagit sans consommer de molécule, est retirée, et envoie 50 et 8 à `s1`, qui les reçoit puis calcule son maximum.

```json
{"t":0.000349,"ev":"reaction","n":3,"w":1,"sol":"s0","regle":"alimenter","in":[],"out":[],"retiree":true}
{"t":0.000349,"ev":"envoi","n":3,"sol":"s0","cible":"s1","out":[{"id":1028,"v":50},{"id":1029,"v":8}]}
{"t":0.000563,"ev":"reception","sol":"s1","mol":[1028,1029]}
{"t":0.00064,"ev":"reaction","n":4,"w":0,"sol":"s1","regle":"max","in":[1028,1029],"out":[{"id":2052,"v":50}]}
```

Une molécule en transit compte dans le nombre de molécules de l'exécution mais n'appartient encore à aucun multi-ensemble. Après un arrêt (garde-fou, interruption), le courrier encore en transit est remis à ses destinataires : la trace se termine alors par des événements `reception` avant `fin`.

## Rejeu et vérification

```sh
chem replay trace.jsonl                          # reconstruction de l'état final
chem replay trace.jsonl --programme prog.chem    # + ré-exécution de chaque réaction
```

Le rejeu refuse toute incohérence :

- molécule absente (consommée deux fois), identifiant réutilisé, sous-solution inconnue ;
- règle qui n'est pas (ou plus) active dans la solution : une règle à usage unique ne réagit qu'une fois par activation ;
- sous-solution liée avant son inertie, ou alors que des molécules lui sont en transit ;
- molécule reçue sans avoir été envoyée (ou reçue deux fois), envoi vers une solution qui n'est pas une sous-solution directe ;
- avec le programme : produits ou effets différents de ceux que la règle calcule, champ `retiree` incohérent avec `replace-one`, valeurs envoyées différentes, cibles d'envoi non distinctes ou confondues avec une sous-solution liée par la même réaction.

Une trace produite en parallèle est ainsi validée comme une exécution séquentielle possible.

## Évolutions

Les lecteurs ignorent les champs inconnus. Un nouvel événement doit être ajouté à `engine/src/trace/evenement.rs` (écriture et lecture), à `engine/src/trace/modele.rs` (validation), à `visualizer/src/trace/` (types et analyse), à `visualizer/src/modele/commandes.ts` (commande annulable) et à ce document ; `version` est incrémentée à chaque ajout. La version 2 est rétrocompatible : toute trace de version 1 reste lisible.
