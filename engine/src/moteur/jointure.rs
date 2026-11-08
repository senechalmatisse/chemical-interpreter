//! Jointures par egalite : indexation des molecules par valeur.
//!
//! Pour une regle a deux reactifs dont la condition contient une egalite
//! entre une expression du premier reactif et une expression du second
//! (`replace (a, b), (c, d) by ... if b == c`), il est inutile d'essayer
//! tous les partenaires d'une ancre : seuls ceux dont la cle vaut celle de
//! l'ancre peuvent reagir. Le reacteur range donc les molecules du lot dans
//! un index par valeur de cle et ne tente que les candidats du bon seau.
//!
//! L'analyse est faite une fois, a la compilation de la regle. Elle ne
//! retient une egalite que si l'indexation ne peut pas changer le
//! comportement du programme :
//!
//! - la condition est une conjonction evaluee de gauche a droite avec court
//!   circuit ; les conjonctions qui precedent l'egalite ne doivent pas
//!   pouvoir echouer (une erreur d'evaluation sur un couple ecarte par
//!   l'index ne serait sinon plus signalee) ;
//! - la cle du partenaire, calculee pour chaque molecule rangee dans
//!   l'index, ne doit pas pouvoir echouer ;
//! - la cle de l'ancre peut echouer : le reacteur revient alors au parcours
//!   complet pour cette ancre, qui evalue la condition comme sans index.
//!
//! `==` est total (comparaison structurelle de deux valeurs quelconques) :
//! deux cles egales au sens de l'index sont exactement les couples pour
//! lesquels l'egalite est vraie.

use super::regle::{Filtre, MotifCompile, ProduitCompile};
use crate::ast::{OpBinaire, OpUnaire};
use crate::evaluateur::{ExprCompilee, Valeur};
use crate::solution::IdMolecule;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Hachage rapide et deterministe (multiplication et rotation) : les cles
/// viennent du programme execute, pas d'une source hostile.
#[derive(Debug, Default, Clone, Copy)]
pub struct HacheurRapide(u64);

impl HacheurRapide {
    #[inline]
    fn ajouter(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for HacheurRapide {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, octets: &[u8]) {
        for morceau in octets.chunks(8) {
            let mut mot = [0u8; 8];
            mot[..morceau.len()].copy_from_slice(morceau);
            self.ajouter(u64::from_le_bytes(mot));
        }
    }

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.ajouter(u64::from(n));
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.ajouter(u64::from(n));
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.ajouter(n);
    }

    #[inline]
    fn write_i64(&mut self, n: i64) {
        self.ajouter(n as u64);
    }

    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.ajouter(n as u64);
    }

    #[inline]
    fn write_isize(&mut self, n: isize) {
        self.ajouter(n as u64);
    }
}

/// Index d'un lot pour une regle a jointure : les molecules qui peuvent
/// tenir le role du partenaire, rangees par valeur de cle.
///
/// Les entrees `(emplacement, molecule)` ne sont jamais mises a jour en
/// place : une molecule consommee laisse une entree perimee, reconnue (son
/// emplacement est vide ou tenu par une autre molecule) et retiree lors
/// d'une consultation ; un produit est ajoute en fin de seau. L'index est
/// reconstruit au debut de chaque passe.
#[derive(Debug, Default)]
pub struct IndexValeurs {
    seaux: HashMap<Valeur, usize, BuildHasherDefault<HacheurRapide>>,
    listes: Vec<Vec<(usize, IdMolecule)>>,
    /// Passe pour laquelle l'index est a jour (0 : jamais construit).
    passe: u64,
}

impl IndexValeurs {
    /// Vrai si l'index a ete construit pour cette passe.
    pub fn a_jour(&self, passe: u64) -> bool {
        self.passe == passe
    }

    /// Vide l'index et le marque construit pour la passe donnee.
    pub fn reinitialiser(&mut self, passe: u64) {
        self.seaux.clear();
        self.listes.clear();
        self.passe = passe;
    }

    /// Range une molecule sous sa cle.
    pub fn inserer(&mut self, cle: Valeur, emplacement: usize, molecule: IdMolecule) {
        let suivant = self.listes.len();
        let seau = *self.seaux.entry(cle).or_insert(suivant);
        if seau == suivant {
            self.listes.push(Vec::new());
        }
        self.listes[seau].push((emplacement, molecule));
    }

    /// Seau d'une cle, s'il existe.
    pub fn seau(&self, cle: &Valeur) -> Option<usize> {
        self.seaux.get(cle).copied()
    }

    /// `k`-ieme entree d'un seau.
    pub fn entree(&self, seau: usize, k: usize) -> Option<(usize, IdMolecule)> {
        self.listes[seau].get(k).copied()
    }

    /// Retire la `k`-ieme entree d'un seau (la derniere prend sa place).
    pub fn retirer(&mut self, seau: usize, k: usize) {
        self.listes[seau].swap_remove(k);
    }

    /// Nombre d'entrees, perimees comprises.
    pub fn len(&self) -> usize {
        self.listes.iter().map(Vec::len).sum()
    }

    /// Vrai si l'index n'a aucune entree.
    pub fn is_empty(&self) -> bool {
        self.listes.iter().all(Vec::is_empty)
    }
}

/// Jointure par egalite d'une regle a deux reactifs : `cle_ancre ==
/// cle_partenaire`.
#[derive(Debug, Clone, PartialEq)]
pub struct Jointure {
    /// Motif (0 ou 1) tenu par l'ancre de la recherche.
    pub ancre: usize,
    /// Motif tenu par le partenaire, cherche dans l'index.
    pub partenaire: usize,
    /// Cle calculee a partir des seules variables de l'ancre ; peut
    /// echouer (repli sur le parcours complet).
    pub cle_ancre: ExprCompilee,
    /// Cle calculee a partir des seules variables du partenaire ; ne peut
    /// pas echouer.
    pub cle_partenaire: ExprCompilee,
    /// Les deux reactifs ont le meme motif et la meme cle, au nom des
    /// variables pres (`x == y`, `k == l` sur des couples) : une molecule a
    /// la meme cle dans les deux roles, et deux molecules ne reagissent
    /// ensemble que si leurs cles sont egales. L'ordonnanceur peut alors
    /// repartir les lots par cle (voir `ordonnanceur::repartition`).
    pub symetrique: bool,
    /// Jointure symetrique dont chaque produit garde la cle des reactifs,
    /// ou ne peut pas etre filtre par le motif (`replace x, y by x if x ==
    /// y`, `replace (k, a), (l, b) by (k, a + b) if k == l`) : les
    /// reactions d'un lot ne creent aucune molecule d'une autre cle.
    pub locale: bool,
}

/// Type connu d'une expression qui ne peut pas echouer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeSur {
    Entier,
    Booleen,
    Chaine,
    NUplet,
    /// Valeur de type inconnu (variable non annotee).
    Quelconque,
}

impl TypeSur {
    fn de_valeur(v: &Valeur) -> Self {
        match v {
            Valeur::Entier(_) => TypeSur::Entier,
            Valeur::Booleen(_) => TypeSur::Booleen,
            Valeur::Chaine(_) => TypeSur::Chaine,
            Valeur::NUplet(_) => TypeSur::NUplet,
        }
    }

    fn de_filtre(f: Filtre) -> Self {
        match f {
            Filtre::Valeur => TypeSur::Quelconque,
            Filtre::Entier => TypeSur::Entier,
            Filtre::Booleen => TypeSur::Booleen,
            Filtre::Chaine => TypeSur::Chaine,
        }
    }
}

/// Emplacements lies par chaque motif, et leur type.
struct Liaisons {
    /// Pour chaque emplacement : motif qui le lie.
    proprietaire: Vec<Option<usize>>,
    /// Pour chaque emplacement : type garanti par le filtre du motif.
    types: Vec<TypeSur>,
}

impl Liaisons {
    fn new(motifs: &[MotifCompile], emplacements: usize) -> Self {
        let mut liaisons = Self {
            proprietaire: vec![None; emplacements],
            types: vec![TypeSur::Quelconque; emplacements],
        };
        for (rang, motif) in motifs.iter().enumerate() {
            liaisons.parcourir(motif, rang);
        }
        liaisons
    }

    fn parcourir(&mut self, motif: &MotifCompile, rang: usize) {
        match motif {
            MotifCompile::Valeur {
                emplacement,
                filtre,
            } => {
                if let Some(p) = self.proprietaire.get_mut(*emplacement) {
                    *p = Some(rang);
                    self.types[*emplacement] = TypeSur::de_filtre(*filtre);
                }
            }
            MotifCompile::NUplet(composantes) => {
                for c in composantes {
                    self.parcourir(c, rang);
                }
            }
            MotifCompile::Solution => {}
        }
    }

    /// Motifs dont l'expression lit des variables : `Ok(None)` pour une
    /// expression constante, `Err(())` si elle lit deux motifs.
    fn motif_lu(&self, expr: &ExprCompilee) -> Result<Option<usize>, ()> {
        let mut lu = None;
        let mut erreur = false;
        visiter_variables(
            expr,
            &mut |i| match (lu, self.proprietaire.get(i).copied().flatten()) {
                (_, None) => erreur = true,
                (None, Some(m)) => lu = Some(m),
                (Some(a), Some(m)) if a != m => erreur = true,
                _ => {}
            },
        );
        if erreur { Err(()) } else { Ok(lu) }
    }

    /// Type de l'expression si elle ne peut pas echouer.
    fn sure(&self, expr: &ExprCompilee) -> Option<TypeSur> {
        match expr {
            ExprCompilee::Constante(v) => Some(TypeSur::de_valeur(v)),
            ExprCompilee::Variable(i) => self.types.get(*i).copied(),
            ExprCompilee::NUplet(elements) => {
                for e in elements {
                    self.sure(e)?;
                }
                Some(TypeSur::NUplet)
            }
            ExprCompilee::Unaire {
                op: OpUnaire::Non,
                operande,
                ..
            } => (self.sure(operande)? == TypeSur::Booleen).then_some(TypeSur::Booleen),
            // `-x` deborde pour le plus petit entier.
            ExprCompilee::Unaire { .. } => None,
            ExprCompilee::Binaire {
                op, gauche, droite, ..
            } => {
                let g = self.sure(gauche)?;
                let d = self.sure(droite)?;
                use OpBinaire::*;
                match op {
                    Egal | Different => Some(TypeSur::Booleen),
                    Inferieur | InferieurEgal | Superieur | SuperieurEgal => {
                        let comparables = (g == TypeSur::Entier && d == TypeSur::Entier)
                            || (g == TypeSur::Chaine && d == TypeSur::Chaine);
                        comparables.then_some(TypeSur::Booleen)
                    }
                    Et | Ou => {
                        (g == TypeSur::Booleen && d == TypeSur::Booleen).then_some(TypeSur::Booleen)
                    }
                    // La concatenation de deux chaines ne peut pas echouer.
                    Plus if g == TypeSur::Chaine && d == TypeSur::Chaine => Some(TypeSur::Chaine),
                    // Depassement, division par zero ou types incompatibles.
                    _ => None,
                }
            }
        }
    }
}

fn visiter_variables(expr: &ExprCompilee, f: &mut impl FnMut(usize)) {
    match expr {
        ExprCompilee::Constante(_) => {}
        ExprCompilee::Variable(i) => f(*i),
        ExprCompilee::NUplet(elements) => {
            for e in elements {
                visiter_variables(e, f);
            }
        }
        ExprCompilee::Unaire { operande, .. } => visiter_variables(operande, f),
        ExprCompilee::Binaire { gauche, droite, .. } => {
            visiter_variables(gauche, f);
            visiter_variables(droite, f);
        }
    }
}

/// Conjonctions de premier niveau, dans leur ordre d'evaluation.
fn conjonctions<'e>(expr: &'e ExprCompilee, sortie: &mut Vec<&'e ExprCompilee>) {
    match expr {
        ExprCompilee::Binaire {
            op: OpBinaire::Et,
            gauche,
            droite,
            ..
        } => {
            conjonctions(gauche, sortie);
            conjonctions(droite, sortie);
        }
        autre => sortie.push(autre),
    }
}

/// Cherche une jointure utilisable dans la condition d'une regle.
pub fn analyser(
    motifs: &[MotifCompile],
    condition: Option<&ExprCompilee>,
    produits: &[ProduitCompile],
    emplacements: usize,
) -> Option<Jointure> {
    if motifs.len() != 2 || motifs.contains(&MotifCompile::Solution) {
        return None;
    }
    let liaisons = Liaisons::new(motifs, emplacements);
    let mut termes = Vec::new();
    conjonctions(condition?, &mut termes);
    for terme in termes {
        if let ExprCompilee::Binaire {
            op: OpBinaire::Egal,
            gauche,
            droite,
            ..
        } = terme
            && let Some(mut jointure) = orienter(&liaisons, gauche, droite)
        {
            jointure.symetrique = symetrique(motifs, &jointure, emplacements);
            jointure.locale = jointure.symetrique
                && conserve_cle(
                    &motifs[jointure.partenaire],
                    &jointure,
                    produits,
                    emplacements,
                );
            return Some(jointure);
        }
        // Une conjonction qui peut echouer ou ne pas etre booleenne arrete
        // l'analyse : l'index l'eviterait sur certains couples.
        if liaisons.sure(terme) != Some(TypeSur::Booleen) {
            return None;
        }
    }
    None
}

/// Oriente une egalite : le partenaire est le cote qui lit un seul motif et
/// ne peut pas echouer ; l'autre cote ne lit que l'autre motif (ou rien).
fn orienter(liaisons: &Liaisons, gauche: &ExprCompilee, droite: &ExprCompilee) -> Option<Jointure> {
    let lus = (
        liaisons.motif_lu(gauche).ok()?,
        liaisons.motif_lu(droite).ok()?,
    );
    // Premier essai : partenaire a droite, puis a gauche.
    for (cle_ancre, cle_partenaire, ancre, partenaire) in [
        (gauche, droite, lus.0, lus.1),
        (droite, gauche, lus.1, lus.0),
    ] {
        let Some(partenaire) = partenaire else {
            continue;
        };
        let ancre_attendue = 1 - partenaire;
        if ancre.is_some_and(|a| a != ancre_attendue) || liaisons.sure(cle_partenaire).is_none() {
            continue;
        }
        return Some(Jointure {
            ancre: ancre_attendue,
            partenaire,
            cle_ancre: cle_ancre.clone(),
            cle_partenaire: cle_partenaire.clone(),
            symetrique: false,
            locale: false,
        });
    }
    None
}

/// Vrai si l'ancre et le partenaire ont le meme motif et la meme cle, a un
/// renommage des variables pres.
fn symetrique(motifs: &[MotifCompile], jointure: &Jointure, emplacements: usize) -> bool {
    let mut renommage = vec![None; emplacements];
    meme_motif(
        &motifs[jointure.ancre],
        &motifs[jointure.partenaire],
        &mut renommage,
    ) && meme_expression(&jointure.cle_ancre, &jointure.cle_partenaire, &renommage)
}

/// Vrai si chaque produit garde la cle des reactifs ou ne peut pas etre
/// filtre par le motif (les envois quittent la solution).
fn conserve_cle(
    motif: &MotifCompile,
    jointure: &Jointure,
    produits: &[ProduitCompile],
    emplacements: usize,
) -> bool {
    let identite: Vec<Option<usize>> = (0..emplacements).map(Some).collect();
    produits.iter().all(|produit| match produit {
        ProduitCompile::Valeur(expression) => {
            let mut substitution = vec![None; emplacements];
            match lier_symbolique(motif, expression, &mut substitution) {
                Liaison::Impossible => true,
                Liaison::Inconnue => false,
                Liaison::Liee => {
                    substituer(&jointure.cle_partenaire, &substitution).is_some_and(|cle| {
                        meme_expression(&cle, &jointure.cle_ancre, &identite)
                            || meme_expression(&cle, &jointure.cle_partenaire, &identite)
                    })
                }
            }
        }
        ProduitCompile::Envoi { .. } => true,
        _ => false,
    })
}

/// Filtrage d'un produit par un motif, sans l'evaluer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Liaison {
    /// Le motif ne peut pas filtrer la valeur du produit.
    Impossible,
    /// Indecidable sans evaluer (variable qui peut tenir un n-uplet...).
    Inconnue,
    /// Chaque emplacement du motif est lie a une sous-expression.
    Liee,
}

fn lier_symbolique<'e>(
    motif: &MotifCompile,
    produit: &'e ExprCompilee,
    substitution: &mut [Option<&'e ExprCompilee>],
) -> Liaison {
    match motif {
        MotifCompile::Valeur { emplacement, .. } => match substitution.get_mut(*emplacement) {
            Some(lie) => {
                *lie = Some(produit);
                Liaison::Liee
            }
            None => Liaison::Inconnue,
        },
        MotifCompile::NUplet(motifs) => match produit {
            ExprCompilee::NUplet(elements) if elements.len() == motifs.len() => {
                let mut resultat = Liaison::Liee;
                for (m, e) in motifs.iter().zip(elements) {
                    match lier_symbolique(m, e, substitution) {
                        Liaison::Impossible => return Liaison::Impossible,
                        Liaison::Inconnue => resultat = Liaison::Inconnue,
                        Liaison::Liee => {}
                    }
                }
                resultat
            }
            ExprCompilee::NUplet(_) => Liaison::Impossible,
            ExprCompilee::Constante(Valeur::NUplet(_)) | ExprCompilee::Variable(_) => {
                Liaison::Inconnue
            }
            // Constante scalaire, operations : jamais un n-uplet.
            _ => Liaison::Impossible,
        },
        MotifCompile::Solution => Liaison::Impossible,
    }
}

/// Remplace les variables par les sous-expressions liees.
fn substituer(
    expression: &ExprCompilee,
    substitution: &[Option<&ExprCompilee>],
) -> Option<ExprCompilee> {
    Some(match expression {
        ExprCompilee::Constante(v) => ExprCompilee::Constante(v.clone()),
        ExprCompilee::Variable(i) => (*substitution.get(*i)?)?.clone(),
        ExprCompilee::NUplet(elements) => ExprCompilee::NUplet(
            elements
                .iter()
                .map(|e| substituer(e, substitution))
                .collect::<Option<_>>()?,
        ),
        ExprCompilee::Unaire { op, operande, span } => ExprCompilee::Unaire {
            op: *op,
            operande: Box::new(substituer(operande, substitution)?),
            span: *span,
        },
        ExprCompilee::Binaire {
            op,
            gauche,
            droite,
            span,
        } => ExprCompilee::Binaire {
            op: *op,
            gauche: Box::new(substituer(gauche, substitution)?),
            droite: Box::new(substituer(droite, substitution)?),
            span: *span,
        },
    })
}

/// Compare deux motifs et note le renommage (emplacement de l'ancre vers
/// emplacement du partenaire).
fn meme_motif(
    ancre: &MotifCompile,
    partenaire: &MotifCompile,
    renommage: &mut [Option<usize>],
) -> bool {
    match (ancre, partenaire) {
        (
            MotifCompile::Valeur {
                emplacement: a,
                filtre: fa,
            },
            MotifCompile::Valeur {
                emplacement: p,
                filtre: fp,
            },
        ) => {
            fa == fp
                && match renommage.get_mut(*a) {
                    Some(libre @ None) => {
                        *libre = Some(*p);
                        true
                    }
                    Some(Some(deja)) => deja == p,
                    None => false,
                }
        }
        (MotifCompile::NUplet(xa), MotifCompile::NUplet(xp)) => {
            xa.len() == xp.len() && xa.iter().zip(xp).all(|(a, p)| meme_motif(a, p, renommage))
        }
        _ => false,
    }
}

/// Compare deux expressions a un renommage des variables pres (les
/// positions dans le source ne comptent pas).
fn meme_expression(
    ancre: &ExprCompilee,
    partenaire: &ExprCompilee,
    renommage: &[Option<usize>],
) -> bool {
    match (ancre, partenaire) {
        (ExprCompilee::Constante(a), ExprCompilee::Constante(p)) => a == p,
        (ExprCompilee::Variable(a), ExprCompilee::Variable(p)) => {
            renommage.get(*a).copied().flatten() == Some(*p)
        }
        (ExprCompilee::NUplet(xa), ExprCompilee::NUplet(xp)) => {
            xa.len() == xp.len()
                && xa
                    .iter()
                    .zip(xp)
                    .all(|(a, p)| meme_expression(a, p, renommage))
        }
        (
            ExprCompilee::Unaire {
                op: oa,
                operande: a,
                ..
            },
            ExprCompilee::Unaire {
                op: op_,
                operande: p,
                ..
            },
        ) => oa == op_ && meme_expression(a, p, renommage),
        (
            ExprCompilee::Binaire {
                op: oa,
                gauche: ga,
                droite: da,
                ..
            },
            ExprCompilee::Binaire {
                op: op_,
                gauche: gp,
                droite: dp,
                ..
            },
        ) => oa == op_ && meme_expression(ga, gp, renommage) && meme_expression(da, dp, renommage),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::analyser_programme;
    use crate::semantique::compiler;
    use crate::source::Source;

    /// Jointure de la regle `r` du programme `let r = ... ; { | r }`.
    fn jointure(regle: &str) -> Option<(usize, usize)> {
        let texte = format!("let r = {regle} ; {{ | r }}");
        let p = analyser_programme(&Source::new("t", &texte)).unwrap();
        let c = compiler(&p, 100).unwrap();
        let r = &c.regles["r"];
        analyser(&r.motifs, r.condition.as_ref(), &r.produits, r.emplacements)
            .map(|j| (j.ancre, j.partenaire))
    }

    #[test]
    fn egalites_indexables() {
        assert_eq!(jointure("replace x, y by x if x == y"), Some((0, 1)));
        assert_eq!(
            jointure("replace (a, b), (c, d) by (a, d) if b == c"),
            Some((0, 1))
        );
        // la cle de l'ancre peut echouer (depassement) : repli prevu
        assert_eq!(jointure("replace x, y by x if x + 1 == y"), Some((0, 1)));
        // la cle du partenaire doit etre sure : on inverse les roles
        assert_eq!(jointure("replace x, y by x if y == x + 1"), Some((0, 1)));
        assert_eq!(jointure("replace x, y by x if x * 2 == y + 1"), None);
        assert_eq!(jointure("replace x, y by y if y + 1 == x"), Some((1, 0)));
        // cle constante : l'ancre est l'autre reactif
        assert_eq!(jointure("replace x, y by y if x == 5"), Some((1, 0)));
        // n-uplets, chaines, conjonction apres l'egalite
        assert_eq!(
            jointure("replace (k, a), (l, b) by (k, a + b) if k == l and a > b"),
            Some((0, 1))
        );
        assert_eq!(
            jointure("replace x:str, y:str by x + y if x + \"!\" == y"),
            Some((0, 1))
        );
    }

    #[test]
    fn conjonctions_qui_precedent_l_egalite() {
        // non annotees, `i < j` peut echouer (types incompatibles)
        assert_eq!(
            jointure("replace (i, a), (j, b) by (i, a) if i < j and a == b"),
            None
        );
        assert_eq!(
            jointure("replace (i:int, a), (j:int, b) by (i, a) if i < j and a == b"),
            Some((0, 1))
        );
        assert_eq!(
            jointure("replace x:int, y:int by x if x > 0 and not (x == 3) and x == y"),
            Some((0, 1))
        );
        assert_eq!(jointure("replace x, y by x if x / 2 > 0 and x == y"), None);
    }

    /// (symetrique, locale) pour la jointure de la regle.
    fn proprietes(regle: &str) -> Option<(bool, bool)> {
        let texte = format!("let r = {regle} ; {{ | r }}");
        let p = analyser_programme(&Source::new("t", &texte)).unwrap();
        let c = compiler(&p, 100).unwrap();
        let r = &c.regles["r"];
        r.jointure.as_ref().map(|j| (j.symetrique, j.locale))
    }

    fn symetrique(regle: &str) -> Option<bool> {
        proprietes(regle).map(|(s, _)| s)
    }

    #[test]
    fn jointures_locales() {
        let locale = |regle: &str| proprietes(regle).map(|(_, l)| l);
        assert_eq!(locale("replace x, y by x if x == y"), Some(true));
        assert_eq!(
            locale("replace (k, a), (l, b) by (k, a + b) if k == l"),
            Some(true)
        );
        assert_eq!(
            locale("replace (k, a), (l, b) by (l, a + b) if k == l"),
            Some(true)
        );
        assert_eq!(
            locale("replace (k, a), (l, b) by (k, a), (l, b - 1) if k == l and b > 0"),
            Some(true)
        );
        // produits que le motif ne peut pas filtrer : scalaire, autre arite
        assert_eq!(
            locale("replace (k, a), (l, b) by a + b, (k, a, b) if k == l"),
            Some(true)
        );
        // nouvelle cle, ou n-uplet de forme inconnue
        assert_eq!(
            locale("replace (k, a), (l, b) by (k + 1, a + b) if k == l"),
            Some(false)
        );
        assert_eq!(
            locale("replace (k, a), (l, b) by (k, a), b if k == l"),
            Some(false)
        );
        assert_eq!(locale("replace x, y by x + 1 if x == y"), Some(false));
        // jointure asymetrique : jamais locale
        assert_eq!(
            locale("replace (a, b), (c, d) by (a, d) if b == c"),
            Some(false)
        );
    }

    #[test]
    fn jointures_symetriques() {
        assert_eq!(symetrique("replace x, y by x if x == y"), Some(true));
        assert_eq!(
            symetrique("replace (k, a), (l, b) by (k, a + b) if k == l"),
            Some(true)
        );
        assert_eq!(
            symetrique("replace (k, a), (l, b) by (k, a) if k == l and a > b"),
            Some(true)
        );
        assert_eq!(
            symetrique("replace (a, b), (c, d) by (a, b) if (a, b) == (c, d)"),
            Some(true)
        );
        assert_eq!(
            symetrique("replace x:str, y:str by x + y if x + \"!\" == y + \"!\""),
            Some(true)
        );
        // la chaine : la cle de l'ancre est sa seconde composante, celle
        // du partenaire sa premiere
        assert_eq!(
            symetrique("replace (a, b), (c, d) by (a, d) if b == c"),
            Some(false)
        );
        assert_eq!(
            symetrique("replace (a, b), (c, d) by (a, b) if (a, b) == (d, c)"),
            Some(false)
        );
        assert_eq!(symetrique("replace x:int, y by x if x == y"), Some(false));
        assert_eq!(symetrique("replace x, y by y if x == 5"), Some(false));
        assert_eq!(symetrique("replace x, (a, b) by x if x == a"), Some(false));
        assert_eq!(symetrique("replace x, y by x if x >= y"), None);
    }

    #[test]
    fn index_par_valeur() {
        let mut index = IndexValeurs::default();
        assert!(!index.a_jour(1));
        index.reinitialiser(1);
        assert!(index.a_jour(1) && index.is_empty());
        index.inserer(Valeur::Entier(5), 0, IdMolecule(10));
        index.inserer(Valeur::chaine("a"), 1, IdMolecule(11));
        index.inserer(Valeur::Entier(5), 2, IdMolecule(12));
        let seau = index.seau(&Valeur::Entier(5)).unwrap();
        assert_eq!(index.entree(seau, 0), Some((0, IdMolecule(10))));
        assert_eq!(index.entree(seau, 1), Some((2, IdMolecule(12))));
        assert_eq!(index.entree(seau, 2), None);
        index.retirer(seau, 0);
        assert_eq!(index.entree(seau, 0), Some((2, IdMolecule(12))));
        assert_eq!(index.len(), 2);
        assert_eq!(index.seau(&Valeur::Entier(6)), None);
        assert!(
            index
                .seau(&Valeur::nuplet(vec![Valeur::Entier(5), Valeur::Entier(5)]))
                .is_none()
        );
        index.reinitialiser(2);
        assert!(index.is_empty() && !index.a_jour(1));
    }

    #[test]
    fn hachage_deterministe() {
        use std::hash::BuildHasher;
        let h = |v: &Valeur| BuildHasherDefault::<HacheurRapide>::default().hash_one(v);
        assert_eq!(h(&Valeur::Entier(42)), h(&Valeur::Entier(42)));
        assert_ne!(h(&Valeur::Entier(42)), h(&Valeur::Entier(43)));
        assert_ne!(h(&Valeur::chaine("ab")), h(&Valeur::chaine("ba")));
    }

    #[test]
    fn sans_jointure() {
        assert_eq!(jointure("replace x, y by x if y mod x == 0"), None);
        assert_eq!(jointure("replace x, y by x if x >= y"), None);
        assert_eq!(jointure("replace x, y by x"), None);
        assert_eq!(jointure("replace x, y by x if x == y or x == 0"), None);
        assert_eq!(jointure("replace x by x if x == 1"), None);
        assert_eq!(jointure("replace x, y, z by x if x == y"), None);
        assert_eq!(jointure("replace (a, b), c by c if a == b"), None);
        assert_eq!(jointure("replace x, s:sol by x if x == 1"), None);
    }
}
