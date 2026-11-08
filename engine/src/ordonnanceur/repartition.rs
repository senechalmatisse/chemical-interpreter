//! Repartition des elements d'une solution entre les lots d'une phase.
//!
//! Par defaut, la distribution est circulaire (l'element `k` va dans le lot
//! `k mod n`) : elle equilibre la charge, meme quand les elements arrivent
//! tries.
//!
//! Une regle a jointure (`replace x, y by x if x == y`, `replace (a, b),
//! (c, d) by (a, d) if b == c`) ne fait reagir que des couples de cles
//! egales : la cle d'ancre de l'un vaut la cle de partenaire de l'autre. En
//! distribution circulaire, les deux molecules d'un couple sont presque
//! toujours dans des lots differents : les lots trouvent peu de couples et
//! le travail est laisse a la fusion, qui est sequentielle. La repartition
//! guidee par la jointure place ensemble les molecules qui peuvent reagir,
//! selon le role de chacune (`roles`) :
//!
//! - une molecule qui n'a qu'une cle (jointure symetrique, ou molecule qui
//!   ne peut tenir qu'un des deux roles) rejoint le groupe de sa cle ;
//! - une molecule qui a deux cles differentes est un maillon de chaine
//!   (`(a, b)` est partenaire pour la cle `a` et ancre pour la cle `b`) :
//!   les maillons sont relies a leur successeur (le maillon dont la cle de
//!   partenaire vaut leur cle d'ancre) et la chaine est coupee en segments,
//!   que chaque lot contracte en parallele (contraction de chaine).
//!
//! Les roles sont calcules en parallele par l'ordonnanceur ; les
//! successeurs et les segments le sont ici, en parallele pour les longues
//! chaines (`segments`), avec un resultat qui ne depend pas du nombre de
//! taches.
//!
//! La repartition n'est qu'une heuristique de placement : toute repartition
//! est correcte, puisque chaque reaction d'un lot est valide dans la
//! solution entiere et que la passe exhaustive du lot final garantit
//! l'inertie. Elle est deterministe : elle ne depend que des valeurs, de
//! l'ordre des elements et d'une graine derivee de la solution et du tour,
//! jamais du minutage.
//!
//! Quand chaque cle est dans un seul lot (`Repartition::cles_separees`),
//! que la regle est la seule de la solution, sans usage unique ni
//! reference `s:ref`, et que ses produits gardent la cle de leurs reactifs
//! (jointure locale), aucune reaction ne peut reunir deux lots : des lots
//! inertes forment une solution inerte, et la phase se passe des fusions
//! (voir [`super`]).

use super::Executeur;
use crate::alea::Alea;
use crate::evaluateur::{ExprCompilee, Valeur};
use crate::moteur::jointure::HacheurRapide;
use crate::moteur::{MotifCompile, RegleCompilee};
use crate::solution::Element;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::hash::{BuildHasher, BuildHasherDefault};
use std::sync::Arc;

/// Nombre moyen de segments de chaine vise par lot : assez pour equilibrer
/// les lots, assez peu pour que la fusion n'ait presque rien a refaire.
const SEGMENTS_PAR_LOT: usize = 8;

/// En dessous de ce nombre de maillons, les successeurs sont construits sur
/// le thread appelant : avec le pool standard, qui demarre un thread par
/// tache, le parallelisme ne gagne qu'au-dela (mesure sur 2 coeurs).
const MIN_MAILLONS_PARALLELES: usize = 1 << 17;

/// Plages d'empreintes par tache pour la construction des successeurs :
/// chaque plage a sa propre table, plus petite qu'une table unique et donc
/// plus souvent dans le cache.
const PLAGES_PAR_TACHE: usize = 4;

/// Maillons par plage, au moins, en moyenne : une table de plus pour moins
/// de maillons couterait plus qu'elle ne ferait gagner en cache.
const MAILLONS_PAR_PLAGE: usize = 8192;

/// Segments parcourus de front par une tache : leurs lectures sont
/// independantes, et le processeur les recouvre au lieu d'attendre la
/// memoire a chaque maillon.
const PARCOURS_DE_FRONT: usize = 16;

/// Absence d'element dans les tableaux d'indices.
const AUCUN: usize = usize::MAX;

/// Role d'un element pour la jointure qui guide la repartition. Les cles
/// sont reduites a leur empreinte de 64 bits : deux cles d'empreintes
/// egales sont traitees comme egales, ce qui ne separe jamais deux
/// molecules de meme cle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// Sous-solution, ou molecule qu'aucun des deux motifs ne filtre.
    Libre,
    /// Une seule cle : jointure symetrique, molecule qui ne peut tenir qu'un
    /// role, ou dont les deux cles sont egales.
    Cle(u64),
    /// Les deux roles avec deux cles differentes (maillon de chaine) :
    /// partenaire des molecules dont la cle d'ancre vaut `partenaire`, ancre
    /// des molecules dont la cle de partenaire vaut `ancre`.
    Maillon { partenaire: u64, ancre: u64 },
}

/// Lot de destination de chaque element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Repartition {
    /// Indice du lot de chaque element, dans l'ordre des elements.
    pub destinations: Vec<usize>,
    /// Vrai si toutes les molecules de meme cle sont dans le meme lot :
    /// aucun groupe reparti entre plusieurs lots, aucun maillon (une chaine
    /// coupee en segments a des couples a cheval sur deux lots). Vrai aussi
    /// quand aucune molecule n'a de cle.
    pub cles_separees: bool,
    /// Segments de chaine places dans les lots.
    pub segments: usize,
}

/// Lot de chaque element, en distribution circulaire.
pub(crate) fn circulaire(elements: usize, lots: usize) -> Vec<usize> {
    (0..elements).map(|k| k % lots.max(1)).collect()
}

/// Regle qui guide la repartition : la premiere regle de la solution dont
/// la jointure est symetrique, sinon la premiere regle a jointure.
pub(crate) fn regle_de_partition(regles: &[Arc<RegleCompilee>]) -> Option<&RegleCompilee> {
    let jointures = || regles.iter().map(|r| &**r).filter(|r| r.jointure.is_some());
    jointures()
        .find(|r| r.jointure.as_ref().is_some_and(|j| j.symetrique))
        .or_else(|| jointures().next())
}

/// Role de chaque element pour la jointure de `regle`. Le calcul porte sur
/// une tranche : l'ordonnanceur le repartit entre les workers.
pub(crate) fn roles(elements: &[Element], regle: &RegleCompilee) -> Vec<Role> {
    let Some(jointure) = regle.jointure.as_ref() else {
        return vec![Role::Libre; elements.len()];
    };
    let motif_ancre = &regle.motifs[jointure.ancre];
    let motif_partenaire = &regle.motifs[jointure.partenaire];
    let mut environnement = vec![Valeur::Entier(0); regle.emplacements];
    let empreinte = BuildHasherDefault::<HacheurRapide>::default();
    let mut cle = |motif: &MotifCompile, expression: &ExprCompilee, valeur: &Valeur| {
        if !motif.lier(valeur, &mut environnement) {
            return None;
        }
        expression
            .evaluer(&environnement)
            .ok()
            .map(|c| empreinte.hash_one(&c))
    };
    elements
        .iter()
        .map(|element| {
            let Element::Molecule(m) = element else {
                return Role::Libre;
            };
            let partenaire = cle(motif_partenaire, &jointure.cle_partenaire, &m.valeur);
            if jointure.symetrique {
                // meme motif et meme cle dans les deux roles
                return partenaire.map_or(Role::Libre, Role::Cle);
            }
            let ancre = cle(motif_ancre, &jointure.cle_ancre, &m.valeur);
            match (partenaire, ancre) {
                (Some(partenaire), Some(ancre)) if partenaire != ancre => {
                    Role::Maillon { partenaire, ancre }
                }
                (Some(c), _) | (None, Some(c)) => Role::Cle(c),
                (None, None) => Role::Libre,
            }
        })
        .collect()
}

/// Lot de chaque element d'apres son role.
///
/// 1. Les elements de meme cle forment un groupe ; les maillons forment des
///    segments de chaine (`segments`, calcules avec les workers de `pool`),
///    coupes en morceaux d'au plus une demi-part de lot.
/// 2. Chaque groupe ou segment qui tient dans un lot (au plus
///    `ceil(n / lots)` elements) y est place en entier : du plus grand au
///    plus petit (tri stable), chacun dans le lot le moins charge.
/// 3. Un groupe plus grand qu'un lot, et les elements libres, sont
///    distribues de facon circulaire : un tres grand groupe est alors reduit
///    dans chaque lot en parallele, puis a la fusion.
///
/// Les elements gardent leur ordre relatif dans chaque lot.
pub(crate) fn repartir(
    roles: &[Role],
    lots: usize,
    graine: u64,
    pool: Executeur<'_>,
) -> Repartition {
    let lots = lots.max(1);
    let capacite = roles.len().div_ceil(lots);
    let mut unite_de: Vec<Option<usize>> = vec![None; roles.len()];
    let mut tailles: Vec<usize> = Vec::new();
    let mut groupes: HashMap<u64, usize, BuildHasherDefault<HacheurRapide>> = HashMap::default();
    let mut maillons = Vec::new();
    for (i, role) in roles.iter().enumerate() {
        match *role {
            Role::Cle(cle) => {
                let suivant = tailles.len();
                let unite = *groupes.entry(cle).or_insert(suivant);
                if unite == suivant {
                    tailles.push(0);
                }
                tailles[unite] += 1;
                unite_de[i] = Some(unite);
            }
            Role::Maillon { .. } => maillons.push(i),
            Role::Libre => {}
        }
    }
    let groupes_separes = tailles.iter().all(|&t| t <= capacite);
    // Un segment trop long est coupe en morceaux d'au plus une demi-part de
    // lot : chaque morceau reste contigu, et les lots restent equilibres.
    let longueur_max = (capacite / 2).max(1);
    let mut nombre_segments = 0;
    for segment in segments(roles, &maillons, lots, graine, pool) {
        for morceau in segment.chunks(longueur_max) {
            let unite = tailles.len();
            tailles.push(morceau.len());
            for &i in morceau {
                unite_de[i] = Some(unite);
            }
            nombre_segments += 1;
        }
    }

    // Unites qui tiennent dans un lot, de la plus grande a la plus petite.
    let mut ordre: Vec<usize> = (0..tailles.len())
        .filter(|&u| tailles[u] <= capacite)
        .collect();
    ordre.sort_by_key(|&u| Reverse(tailles[u]));
    let mut charges: BinaryHeap<Reverse<(usize, usize)>> =
        (0..lots).map(|lot| Reverse((0, lot))).collect();
    let mut lot_de_l_unite: Vec<Option<usize>> = vec![None; tailles.len()];
    for unite in ordre {
        let Some(Reverse((charge, lot))) = charges.pop() else {
            break;
        };
        lot_de_l_unite[unite] = Some(lot);
        charges.push(Reverse((charge + tailles[unite], lot)));
    }

    let mut suivant = 0;
    let destinations = unite_de
        .into_iter()
        .map(|unite| match unite.and_then(|u| lot_de_l_unite[u]) {
            Some(lot) => lot,
            None => {
                let lot = suivant % lots;
                suivant += 1;
                lot
            }
        })
        .collect();
    Repartition {
        destinations,
        cles_separees: groupes_separes && maillons.is_empty(),
        segments: nombre_segments,
    }
}

/// Segments de chaine formes par les maillons (indices dans `roles`, dans
/// l'ordre des elements).
///
/// Chaque maillon est relie a un successeur : le premier maillon encore
/// libre dont la cle de partenaire vaut sa cle d'ancre, les maillons
/// choisissant dans l'ordre des elements (un maillon n'a donc jamais deux
/// predecesseurs, et les liens forment des chaines et des cycles
/// disjoints). Les chaines sont coupees en des reperes : les debuts de
/// chaine (maillons sans predecesseur) et des maillons tires au hasard,
/// assez nombreux pour donner en moyenne `SEGMENTS_PAR_LOT` segments par
/// lot. Un segment va d'un repere au repere suivant ; un cycle sans repere
/// recoit le premier de ses maillons comme repere.
///
/// A partir de `MIN_MAILLONS_PARALLELES` maillons, le calcul est partage en
/// une tache par lot (`segments_en_taches`).
fn segments(
    roles: &[Role],
    maillons: &[usize],
    lots: usize,
    graine: u64,
    pool: Executeur<'_>,
) -> Vec<Vec<usize>> {
    let taches = if maillons.len() >= MIN_MAILLONS_PARALLELES {
        lots
    } else {
        1
    };
    segments_en_taches(roles, maillons, lots, graine, pool, taches)
}

/// Segments de chaine (voir `segments`) calcules en `taches` taches, en
/// quatre etapes paralleles :
///
/// 1. chaque tranche de maillons range les siens par plage d'empreintes,
///    une fois selon leur cle de partenaire et une fois selon leur cle
///    d'ancre (`ranger`) ;
/// 2. chaque plage relie ses maillons, avec sa propre table (`relier`) :
///    un maillon et son successeur ont une cle commune, donc la meme
///    plage, et chaque plage voit ses maillons dans l'ordre des elements ;
/// 3. chaque tranche recopie le successeur de ses maillons dans sa partie
///    du tableau des successeurs, et choisit ses reperes (`placer`) ;
/// 4. chaque tranche parcourt les segments qui partent de ses reperes
///    (`parcourir`). Un maillon n'a qu'un predecesseur : deux parcours ne
///    se croisent jamais.
///
/// Aucune tache n'ecrit dans les donnees d'une autre : chaque plage a ses
/// propres resultats, chaque tranche sa partie du tableau, et les coeurs ne
/// se disputent pas les memes lignes de cache. Le tirage d'un repere ne
/// depend que de la graine et de l'element (`Alea::au_rang`), et les
/// segments sont rendus dans l'ordre de leur repere : le resultat est celui
/// d'un calcul sequentiel, quel que soit le nombre de taches.
fn segments_en_taches(
    roles: &[Role],
    maillons: &[usize],
    lots: usize,
    graine: u64,
    pool: Executeur<'_>,
    taches: usize,
) -> Vec<Vec<usize>> {
    if maillons.is_empty() {
        return Vec::new();
    }
    let n = roles.len();
    let taille = maillons.len().div_ceil(taches.clamp(1, maillons.len()));
    let mut tranches: Vec<&[usize]> = maillons.chunks(taille).collect();

    // Etapes 1 et 2 : successeurs, plage par plage.
    let plages = (maillons.len() / MAILLONS_PAR_PLAGE)
        .clamp(tranches.len(), tranches.len() * PLAGES_PAR_TACHE);
    let rangements = pool.pour_chaque_mut(&mut tranches, &|tranche: &mut &[usize]| {
        ranger(roles, tranche, plages)
    });
    let mut indices: Vec<usize> = (0..plages).collect();
    let liens = pool.pour_chaque_mut(&mut indices, &|plage: &mut usize| {
        relier(&rangements, *plage)
    });

    // Etape 3 : la partie d'une tranche va de son premier maillon au
    // premier maillon de la tranche suivante.
    let mut successeur = vec![AUCUN; n];
    let mut parties = Vec::with_capacity(tranches.len());
    let mut reste: &mut [usize] = &mut successeur[tranches[0][0]..];
    for (rang, tranche) in tranches.iter().enumerate() {
        let debut = tranche[0];
        let fin = tranches.get(rang + 1).map_or(n, |suivante| suivante[0]);
        let (partie, suite) = std::mem::take(&mut reste).split_at_mut(fin - debut);
        reste = suite;
        parties.push(Partie {
            rang,
            tranche,
            debut,
            successeurs: partie,
        });
    }
    // Un repere est un maillon sans predecesseur, ou un maillon tire au
    // hasard.
    let probabilite = (SEGMENTS_PAR_LOT * lots.max(1)) as f64 / maillons.len() as f64;
    let tire = |i: usize| Alea::au_rang(graine, i as u64).probabilite(probabilite);
    let mut departs = pool.pour_chaque_mut(&mut parties, &|partie: &mut Partie<'_>| {
        placer(partie, &rangements, &liens, &tire)
    });
    // Les parties rendent le tableau des successeurs, et la memoire des
    // etapes 1 et 2 est liberee avant les parcours.
    drop((parties, liens, rangements));

    // Etape 4 : parcours depuis les reperes.
    let parcours = pool.pour_chaque_mut(&mut departs, &|departs: &mut Vec<usize>| {
        parcourir(departs, &successeur, &tire)
    });
    let mut resultat: Vec<Vec<usize>> = parcours.into_iter().flatten().collect();

    // Un maillon qu'aucun parcours n'a couvert est sur un cycle sans
    // repere (en remontant ses predecesseurs, on ne rencontre jamais de
    // repere) : le premier maillon du cycle en devient le repere.
    let couverts: usize = resultat.iter().map(Vec::len).sum();
    if couverts < maillons.len() {
        let mut couvert = vec![false; n];
        for &i in resultat.iter().flatten() {
            couvert[i] = true;
        }
        for &depart in maillons {
            if couvert[depart] {
                continue;
            }
            couvert[depart] = true;
            let mut segment = vec![depart];
            let mut j = successeur[depart];
            while j != AUCUN && !couvert[j] {
                couvert[j] = true;
                segment.push(j);
                j = successeur[j];
            }
            resultat.push(segment);
        }
    }
    resultat
}

/// Maillons d'une tranche ranges par plage d'empreintes : `(cle, element)`,
/// dans l'ordre des elements (etape 1).
struct Rangement {
    /// Selon la cle de partenaire.
    partenaires: Vec<Vec<(u64, usize)>>,
    /// Selon la cle d'ancre.
    ancres: Vec<Vec<(u64, usize)>>,
}

/// Resultats d'une plage (etape 2), dans l'ordre des tranches puis de leur
/// rangement.
struct Liens {
    /// Successeur de chaque maillon range selon sa cle d'ancre (`AUCUN` :
    /// pas de successeur).
    successeurs: Vec<usize>,
    /// Pour chaque maillon range selon sa cle de partenaire : vrai s'il est
    /// le successeur d'un autre maillon.
    pris: Vec<bool>,
}

/// Tranche de l'etape 3 : ses maillons, et la partie du tableau des
/// successeurs qui couvre leurs indices.
struct Partie<'a> {
    /// Rang de la tranche.
    rang: usize,
    /// Maillons de la tranche.
    tranche: &'a [usize],
    /// Indice du premier element de la partie.
    debut: usize,
    /// Successeurs des elements `debut..`, jusqu'au premier maillon de la
    /// tranche suivante.
    successeurs: &'a mut [usize],
}

/// Plage d'une empreinte parmi `plages` : ses bits de poids fort, les mieux
/// melanges par la multiplication du hacheur.
fn plage(cle: u64, plages: usize) -> usize {
    ((u128::from(cle) * plages as u128) >> 64) as usize
}

/// Range les maillons d'une tranche par plage (etape 1).
fn ranger(roles: &[Role], tranche: &[usize], plages: usize) -> Rangement {
    // Capacite prevue un peu au-dessus de la moyenne par plage.
    let prevue = tranche.len() / plages + tranche.len() / (4 * plages) + 8;
    let vides = || (0..plages).map(|_| Vec::with_capacity(prevue)).collect();
    let mut rangement = Rangement {
        partenaires: vides(),
        ancres: vides(),
    };
    for &i in tranche {
        let Role::Maillon { partenaire, ancre } = roles[i] else {
            unreachable!("seuls les maillons sont relies");
        };
        rangement.partenaires[plage(partenaire, plages)].push((partenaire, i));
        rangement.ancres[plage(ancre, plages)].push((ancre, i));
    }
    rangement
}

/// Relie les maillons d'une plage a leur successeur (etape 2).
fn relier(rangements: &[Rangement], plage: usize) -> Liens {
    // Seaux par cle de partenaire : listes chainees dans l'ordre des
    // elements (`membres[q]` est l'element de la position `q`).
    let total: usize = rangements.iter().map(|r| r.partenaires[plage].len()).sum();
    let mut membres = Vec::with_capacity(total);
    for rangement in rangements {
        membres.extend(rangement.partenaires[plage].iter().map(|&(_, i)| i));
    }
    let mut suivant_du_seau = vec![AUCUN; total];
    let mut tete: HashMap<u64, usize, BuildHasherDefault<HacheurRapide>> =
        HashMap::with_capacity_and_hasher(total, BuildHasherDefault::default());
    let mut q = total;
    for rangement in rangements.iter().rev() {
        for &(cle, _) in rangement.partenaires[plage].iter().rev() {
            q -= 1;
            suivant_du_seau[q] = tete.insert(cle, q).unwrap_or(AUCUN);
        }
    }
    // Chaque maillon prend le premier maillon libre du seau de sa cle
    // d'ancre ; il n'est jamais dans ce seau : ses deux cles different.
    let ancres: usize = rangements.iter().map(|r| r.ancres[plage].len()).sum();
    let mut successeurs = Vec::with_capacity(ancres);
    let mut pris = vec![false; total];
    for rangement in rangements {
        for &(cle, _) in &rangement.ancres[plage] {
            let mut successeur = AUCUN;
            if let Some(premier) = tete.get_mut(&cle)
                && *premier != AUCUN
            {
                let q = *premier;
                *premier = suivant_du_seau[q];
                pris[q] = true;
                successeur = membres[q];
            }
            successeurs.push(successeur);
        }
    }
    Liens { successeurs, pris }
}

/// Recopie le successeur des maillons d'une tranche dans sa partie, et
/// retourne ses reperes dans l'ordre des elements (etape 3). Dans les
/// resultats d'une plage, ceux de la tranche suivent ceux des tranches
/// precedentes.
fn placer(
    partie: &mut Partie<'_>,
    rangements: &[Rangement],
    liens: &[Liens],
    tire: &impl Fn(usize) -> bool,
) -> Vec<usize> {
    let precedentes = &rangements[..partie.rang];
    let rangement = &rangements[partie.rang];
    let mut pris = vec![false; partie.successeurs.len()];
    for (plage, lien) in liens.iter().enumerate() {
        let decalage: usize = precedentes.iter().map(|r| r.ancres[plage].len()).sum();
        for (&(_, i), &j) in rangement.ancres[plage]
            .iter()
            .zip(&lien.successeurs[decalage..])
        {
            partie.successeurs[i - partie.debut] = j;
        }
        let decalage: usize = precedentes.iter().map(|r| r.partenaires[plage].len()).sum();
        for (&(_, i), &p) in rangement.partenaires[plage]
            .iter()
            .zip(&lien.pris[decalage..])
        {
            pris[i - partie.debut] = p;
        }
    }
    partie
        .tranche
        .iter()
        .copied()
        .filter(|&i| !pris[i - partie.debut] || tire(i))
        .collect()
}

/// Segments qui partent des reperes `departs`, dans leur ordre (etape 4) :
/// chaque segment suit les successeurs jusqu'au prochain repere, ou
/// jusqu'a la fin de la chaine. Un maillon atteint a un predecesseur : seul
/// le tirage (`tire`) peut en faire un repere. Jusqu'a `PARCOURS_DE_FRONT`
/// segments avancent ensemble, d'un maillon chacun a tour de role.
fn parcourir(
    departs: &[usize],
    successeur: &[usize],
    tire: &impl Fn(usize) -> bool,
) -> Vec<Vec<usize>> {
    let mut segments: Vec<Vec<usize>> = departs.iter().map(|&d| vec![d]).collect();
    // (segment, dernier maillon atteint)
    let mut actifs: Vec<(usize, usize)> = Vec::with_capacity(PARCOURS_DE_FRONT);
    let mut prochain = 0;
    loop {
        while actifs.len() < PARCOURS_DE_FRONT && prochain < departs.len() {
            actifs.push((prochain, departs[prochain]));
            prochain += 1;
        }
        if actifs.is_empty() {
            return segments;
        }
        let mut k = 0;
        while k < actifs.len() {
            let (segment, maillon) = actifs[k];
            let suivant = successeur[maillon];
            if suivant == AUCUN || tire(suivant) {
                actifs.swap_remove(k);
            } else {
                segments[segment].push(suivant);
                actifs[k].1 = suivant;
                k += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ordonnanceur::{FabriqueParallelisme, Parallelisme, ParallelismeStandard};
    use crate::parser::analyser_programme;
    use crate::semantique::compiler;
    use crate::solution::{IdMolecule, Molecule};
    use crate::source::Source;
    use std::collections::VecDeque;

    /// Pool standard de `workers` workers.
    fn pool(workers: usize) -> Box<dyn Parallelisme> {
        ParallelismeStandard.creer(workers)
    }

    fn regle(texte: &str) -> RegleCompilee {
        let programme = format!("let r = {texte} ; {{ | r }}");
        let p = analyser_programme(&Source::new("t", &programme)).unwrap();
        let c = compiler(&p, 100).unwrap();
        (*c.regles["r"]).clone()
    }

    fn molecules(valeurs: Vec<Valeur>) -> Vec<Element> {
        valeurs
            .into_iter()
            .enumerate()
            .map(|(k, valeur)| {
                Element::Molecule(Molecule {
                    id: IdMolecule(k as u64 + 1),
                    valeur,
                })
            })
            .collect()
    }

    fn couple(k: i64, v: i64) -> Valeur {
        Valeur::nuplet(vec![Valeur::Entier(k), Valeur::Entier(v)])
    }

    /// Lots d'apres les roles calcules pour `regle`.
    fn repartition(texte: &str, valeurs: Vec<Valeur>, lots: usize) -> Repartition {
        let regle = regle(texte);
        let workers = pool(lots);
        repartir(
            &roles(&molecules(valeurs), &regle),
            lots,
            7,
            Executeur::new(&*workers),
        )
    }

    /// Calcul sequentiel de reference des segments, ecrit sans recherche de
    /// performance : une file par cle de partenaire, les maillons
    /// reclamant leur successeur dans l'ordre des elements, puis les
    /// parcours depuis les reperes et ceux des cycles sans repere.
    fn segments_de_reference(
        roles: &[Role],
        maillons: &[usize],
        lots: usize,
        graine: u64,
    ) -> Vec<Vec<usize>> {
        let n = roles.len();
        let cles = |i: usize| match roles[i] {
            Role::Maillon { partenaire, ancre } => (partenaire, ancre),
            _ => unreachable!(),
        };
        let mut files: HashMap<u64, VecDeque<usize>> = HashMap::new();
        for &i in maillons {
            files.entry(cles(i).0).or_default().push_back(i);
        }
        let mut successeur = vec![AUCUN; n];
        let mut pris = vec![false; n];
        for &i in maillons {
            if let Some(j) = files.get_mut(&cles(i).1).and_then(VecDeque::pop_front) {
                successeur[i] = j;
                pris[j] = true;
            }
        }
        let probabilite = (SEGMENTS_PAR_LOT * lots) as f64 / maillons.len() as f64;
        let repere: Vec<bool> = (0..n)
            .map(|i| !pris[i] || Alea::au_rang(graine, i as u64).probabilite(probabilite))
            .collect();
        let mut visite = vec![false; n];
        let mut resultat = Vec::new();
        for passage in 0..2 {
            for &depart in maillons {
                if visite[depart] || (passage == 0 && !repere[depart]) {
                    continue;
                }
                visite[depart] = true;
                let mut segment = vec![depart];
                let mut j = successeur[depart];
                while j != AUCUN && !visite[j] && (passage == 1 || !repere[j]) {
                    visite[j] = true;
                    segment.push(j);
                    j = successeur[j];
                }
                resultat.push(segment);
            }
        }
        resultat
    }

    /// Empreinte d'une cle entiere, comme celles calculees par `roles`.
    fn empreinte(k: i64) -> u64 {
        BuildHasherDefault::<HacheurRapide>::default().hash_one(Valeur::Entier(k))
    }

    /// Maillon `(a, b)` de la chaine `replace (a, b), (c, d) ... if b == c`.
    fn maillon(a: i64, b: i64) -> Role {
        Role::Maillon {
            partenaire: empreinte(a),
            ancre: empreinte(b),
        }
    }

    const CHAINE: &str = "replace (a, b), (c, d) by (a, d) if b == c";

    #[test]
    fn circulaire_equilibre() {
        assert_eq!(circulaire(5, 2), vec![0, 1, 0, 1, 0]);
        assert_eq!(circulaire(3, 1), vec![0, 0, 0]);
    }

    #[test]
    fn regle_choisie() {
        let chaine = Arc::new(regle(CHAINE));
        let somme = Arc::new(regle("replace (k, a), (l, b) by (k, a + b) if k == l"));
        let maximum = Arc::new(regle("replace x, y by x if x >= y"));
        assert!(regle_de_partition(std::slice::from_ref(&maximum)).is_none());
        let regles = [maximum.clone(), chaine.clone()];
        assert!(std::ptr::eq(regle_de_partition(&regles).unwrap(), &*chaine));
        let regles = [chaine, somme.clone()];
        assert!(std::ptr::eq(regle_de_partition(&regles).unwrap(), &*somme));
    }

    #[test]
    fn roles_des_molecules() {
        let elements = molecules(vec![couple(1, 2), couple(3, 3), Valeur::Entier(4)]);
        let chaine = roles(&elements, &regle(CHAINE));
        assert!(matches!(chaine[0], Role::Maillon { .. }));
        assert!(
            matches!(chaine[1], Role::Cle(_)),
            "(3, 3) : deux cles egales"
        );
        assert_eq!(chaine[2], Role::Libre);
        let somme = roles(
            &elements,
            &regle("replace (k, a), (l, b) by (k, a + b) if k == l"),
        );
        assert!(matches!(somme[0], Role::Cle(_)));
        assert_eq!(somme[2], Role::Libre);
        // formes distinctes : chaque molecule ne tient qu'un role
        let formes = roles(
            &molecules(vec![
                couple(5, 0),
                Valeur::nuplet(vec![
                    Valeur::Entier(5),
                    Valeur::Entier(1),
                    Valeur::Entier(2),
                ]),
            ]),
            &regle("replace (k, v), (l, w, x) by (k, v + w) if k == l"),
        );
        assert_eq!(formes[0], formes[1], "meme cle, meme groupe");
        assert!(matches!(formes[0], Role::Cle(_)));
    }

    #[test]
    fn meme_cle_meme_lot_et_charge_equilibree() {
        // 40 cles, de 1 a 4 molecules chacune
        let mut valeurs = Vec::new();
        for k in 0..40 {
            for v in 0..=(k % 4) {
                valeurs.push(couple(k, v));
            }
        }
        let somme = "replace (k, a), (l, b) by (k, a + b) if k == l";
        let r = repartition(somme, valeurs.clone(), 4);
        assert!(r.cles_separees);
        assert_eq!(r.segments, 0);
        let mut lot_de_la_cle = HashMap::new();
        for (valeur, lot) in valeurs.iter().zip(&r.destinations) {
            let Valeur::NUplet(composantes) = valeur else {
                unreachable!()
            };
            let deja = *lot_de_la_cle.entry(composantes[0].clone()).or_insert(*lot);
            assert_eq!(
                deja, *lot,
                "toutes les molecules d'une cle dans le meme lot"
            );
        }
        let mut charges = [0usize; 4];
        for &lot in &r.destinations {
            charges[lot] += 1;
        }
        let (min, max) = (charges.iter().min().unwrap(), charges.iter().max().unwrap());
        assert!(max - min <= 4, "charges {charges:?}");
        assert_eq!(repartition(somme, valeurs, 4).destinations, r.destinations);
    }

    #[test]
    fn grands_groupes_et_elements_libres_repartis() {
        let mut valeurs: Vec<Valeur> = (0..12).map(|_| Valeur::Entier(7)).collect();
        valeurs.extend((0..4).map(|k| Valeur::chaine(&k.to_string())));
        // 16 elements, 2 lots de 8 : le groupe de 12 ne tient pas dans un lot
        let r = repartition("replace x:int, y:int by x if x == y", valeurs, 2);
        assert!(!r.cles_separees);
        let lots = r.destinations;
        assert_eq!(lots.iter().filter(|&&l| l == 0).count(), 8);
        assert_eq!(lots[..12].iter().filter(|&&l| l == 0).count(), 6);
        // chaines : le motif x:int ne les filtre pas, distribution circulaire
        assert_eq!(&lots[12..], &[0, 1, 0, 1]);
    }

    #[test]
    fn chaine_coupee_en_segments() {
        // chaine (0, 1), (1, 2), ..., (399, 400), melangee
        let mut maillons: Vec<Valeur> = (0..400).map(|i| couple(i, i + 1)).collect();
        Alea::new(3).melanger(&mut maillons);
        let r = repartition(CHAINE, maillons.clone(), 4);
        assert!(!r.cles_separees);
        assert!(
            r.segments >= 4 && r.segments <= 400,
            "{} segments",
            r.segments
        );
        // deux maillons consecutifs sont dans le meme lot, sauf aux coupures
        let lot_du_debut: HashMap<i64, usize> = maillons
            .iter()
            .zip(&r.destinations)
            .map(|(v, &lot)| {
                let Valeur::NUplet(c) = v else { unreachable!() };
                let Valeur::Entier(a) = c[0] else {
                    unreachable!()
                };
                (a, lot)
            })
            .collect();
        let coupures = (0..399)
            .filter(|i| lot_du_debut[i] != lot_du_debut[&(i + 1)])
            .count();
        assert!(
            coupures < r.segments,
            "{coupures} coupures, {} segments",
            r.segments
        );
        // charge raisonnable : aucun lot vide, aucun au-dela de deux fois la part
        let mut charges = [0usize; 4];
        for &lot in &r.destinations {
            charges[lot] += 1;
        }
        assert!(
            charges.iter().all(|&c| c > 0 && c <= 200),
            "charges {charges:?}"
        );
        assert_eq!(
            repartition(CHAINE, maillons, 4).destinations,
            r.destinations
        );
    }

    #[test]
    fn cycles_et_branches() {
        // cycle (0, 1), (1, 2), (2, 0) ; branche (5, 6), (6, 7), (6, 8)
        let valeurs = vec![
            couple(0, 1),
            couple(1, 2),
            couple(2, 0),
            couple(5, 6),
            couple(6, 7),
            couple(6, 8),
        ];
        let r = repartition(CHAINE, valeurs.clone(), 2);
        assert_eq!(r.destinations.len(), valeurs.len());
        let regle = regle(CHAINE);
        let roles = roles(&molecules(valeurs), &regle);
        let maillons: Vec<usize> = (0..roles.len()).collect();
        let workers = pool(2);
        let segments = segments(&roles, &maillons, 2, 7, Executeur::new(&*workers));
        let mut couverts: Vec<usize> = segments.concat();
        couverts.sort_unstable();
        assert_eq!(couverts, maillons, "chaque maillon dans un seul segment");
    }

    #[test]
    fn sans_jointure_circulaire() {
        let elements = molecules((0..6).map(Valeur::Entier).collect());
        let maximum = regle("replace x, y by x if x >= y");
        assert_eq!(roles(&elements, &maximum), vec![Role::Libre; 6]);
        let workers = pool(3);
        let r = repartir(&roles(&elements, &maximum), 3, 7, Executeur::new(&*workers));
        assert_eq!(r.destinations, circulaire(6, 3));
        // aucune cle : aucune n'est separee
        assert!(r.cles_separees);
    }

    #[test]
    fn successeurs_paralleles_comme_en_sequentiel() {
        let mut alea = Alea::new(5);
        let mut cas: Vec<(&str, Vec<Role>)> = Vec::new();
        // longue chaine melangee, parmi des elements qui ne sont pas des
        // maillons (cles simples, elements libres)
        let mut chaine: Vec<Role> = (0..3000).map(|i| maillon(i, i + 1)).collect();
        chaine.extend((0..300).map(|k| Role::Cle(empreinte(k))));
        chaine.extend((0..300).map(|_| Role::Libre));
        alea.melanger(&mut chaine);
        cas.push(("chaine", chaine));
        // cles tirees parmi peu de valeurs : branches, maillons disputes,
        // cycles de toutes tailles
        for valeurs in [40, 400, 4000] {
            let tirees = (0..3000)
                .map(|_| {
                    let a = alea.entre(0, valeurs - 1);
                    let b = (a + alea.entre(1, valeurs - 1)) % valeurs;
                    maillon(a, b)
                })
                .collect();
            cas.push(("tirees", tirees));
        }
        // cycles courts sans debut de chaine, et maillons isoles
        let mut cycles: Vec<Role> = (0..500).map(|i| maillon(i, (i + 1) % 500)).collect();
        cycles.extend((0..100).map(|c| maillon(1000 + 2 * c, 1001 + 2 * c)));
        cycles.extend((0..100).map(|c| maillon(1001 + 2 * c, 1000 + 2 * c)));
        cycles.push(maillon(5000, 5001));
        alea.melanger(&mut cycles);
        cas.push(("cycles", cycles));
        cas.push(("un_maillon", vec![maillon(1, 2)]));

        for (nom, roles) in &cas {
            let maillons: Vec<usize> = (0..roles.len())
                .filter(|&i| matches!(roles[i], Role::Maillon { .. }))
                .collect();
            for lots in [1, 2, 4] {
                let attendu = segments_de_reference(roles, &maillons, lots, 11);
                let mut couverts: Vec<usize> = attendu.concat();
                couverts.sort_unstable();
                assert_eq!(couverts, maillons, "{nom} : chaque maillon une fois");
                for workers in [1, 2, 4] {
                    let p = pool(workers);
                    for taches in [1, 2, 3, 8] {
                        assert_eq!(
                            segments_en_taches(
                                roles,
                                &maillons,
                                lots,
                                11,
                                Executeur::new(&*p),
                                taches
                            ),
                            attendu,
                            "{nom}, {lots} lot(s), {workers} worker(s), {taches} tache(s)"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn grande_chaine_partagee_entre_les_lots() {
        // au-dela du seuil : une tache par lot
        let longueur = MIN_MAILLONS_PARALLELES as i64 + 1000;
        let mut roles: Vec<Role> = (0..longueur).map(|i| maillon(i, i + 1)).collect();
        Alea::new(8).melanger(&mut roles);
        let maillons: Vec<usize> = (0..roles.len()).collect();
        let p = pool(4);
        let segments = segments(&roles, &maillons, 4, 3, Executeur::new(&*p));
        assert_eq!(segments, segments_de_reference(&roles, &maillons, 4, 3));
        // environ 8 segments par lot, une seule chaine
        assert!(
            (8..=96).contains(&segments.len()),
            "{} segments",
            segments.len()
        );
        let r = repartir(&roles, 4, 3, Executeur::new(&*p));
        assert_eq!(r.segments, segments.len(), "aucun segment trop long");
        let mut charges = [0usize; 4];
        for &lot in &r.destinations {
            charges[lot] += 1;
        }
        let part = roles.len() / 4;
        assert!(
            charges.iter().all(|&c| c >= part / 2 && c <= 2 * part),
            "charges {charges:?}"
        );
    }

    /// Mesure de la construction des successeurs et des segments, sur une
    /// chaine melangee : `cargo test --release --lib mesure_des_successeurs
    /// -- --ignored --nocapture` (mediane de 31 executions, 1 tache puis une
    /// tache par lot, 2 et 4 lots).
    #[test]
    #[ignore]
    fn mesure_des_successeurs() {
        use std::time::Instant;
        const ESSAIS: usize = 31;
        for longueur in [30_000, 100_000, 300_000] {
            let mut roles: Vec<Role> = (0..longueur).map(|i| maillon(i, i + 1)).collect();
            Alea::new(3).melanger(&mut roles);
            let maillons: Vec<usize> = (0..roles.len()).collect();
            for lots in [2, 4] {
                let workers = pool(lots);
                let executeur = Executeur::new(&*workers);
                let mut durees = [[0.0f64; ESSAIS]; 2];
                for essai in 0..ESSAIS {
                    for (serie, taches) in [1, lots].into_iter().enumerate() {
                        let debut = Instant::now();
                        let segments =
                            segments_en_taches(&roles, &maillons, lots, 7, executeur, taches);
                        durees[serie][essai] = debut.elapsed().as_secs_f64() * 1e3;
                        assert!(!segments.is_empty());
                    }
                }
                let mediane = |serie: &mut [f64; ESSAIS]| {
                    serie.sort_by(f64::total_cmp);
                    serie[ESSAIS / 2]
                };
                let (une, plusieurs) = (mediane(&mut durees[0]), mediane(&mut durees[1]));
                eprintln!(
                    "{longueur:>7} maillons, {lots} lots : 1 tache {une:.3} ms, \
                     {lots} taches {plusieurs:.3} ms (x{:.2})",
                    une / plusieurs
                );
            }
        }
    }
}
