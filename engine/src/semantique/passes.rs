//! Passes de verification avant execution.
//!
//! Chaque passe est independante et n'a qu'une responsabilite ; elles sont
//! enchainees par l'[`super::Analyseur`] (chaine de responsabilite en mode
//! pipeline : chaque maillon traite le programme et transmet au suivant).

use super::types::{EnvironnementTypes, TypeStatique, inferer};
use crate::ast::visiteur::{CollecteurVariables, Visiteur, parcourir_element, parcourir_solution};
use crate::ast::*;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::source::Span;
use std::collections::{HashMap, HashSet};

/// Une verification du programme.
pub trait Passe: Send + Sync {
    /// Nom de la passe (documentation, mise au point).
    fn nom(&self) -> &'static str;
    /// Analyse le programme et ajoute ses diagnostics.
    fn analyser(&self, programme: &Programme, diagnostics: &mut Diagnostics);
}

/// Distance d'edition (suggestions de noms).
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut precedente: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut courante = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cout = usize::from(ca != *cb);
            courante[j + 1] = (precedente[j] + cout)
                .min(precedente[j + 1] + 1)
                .min(courante[j] + 1);
        }
        precedente = courante;
    }
    precedente[b.len()]
}

/// Nom le plus proche parmi des candidats, s'il est assez proche.
pub fn suggestion<'a>(nom: &str, candidats: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    candidats
        .map(|c| (distance(nom, c), c))
        .filter(|(d, c)| *d <= 2.max(c.len() / 3) && *d < nom.len().max(1))
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

/// Noms de regles uniques.
pub struct NomsUniques;

impl Passe for NomsUniques {
    fn nom(&self) -> &'static str {
        "noms-uniques"
    }

    fn analyser(&self, programme: &Programme, diagnostics: &mut Diagnostics) {
        let mut vus: HashMap<&str, Span> = HashMap::new();
        for d in &programme.definitions {
            if let Some(premiere) = vus.get(d.nom.nom.as_str()) {
                diagnostics.ajouter(
                    Diagnostic::erreur(
                        "E0301",
                        format!("la regle '{}' est definie plusieurs fois", d.nom.nom),
                    )
                    .etiquette(d.nom.span, "nouvelle definition")
                    .etiquette(*premiere, "premiere definition")
                    .aide("renommez l'une des deux regles"),
                );
            } else {
                vus.insert(&d.nom.nom, d.nom.span);
            }
        }
    }
}

/// Regles activees definies ; regles definies utilisees.
pub struct ReglesDefinies;

struct ReferencesRegles<'a> {
    definies: &'a HashSet<&'a str>,
    /// Regles `replace-one` : deux activations valent deux usages.
    uniques: &'a HashSet<&'a str>,
    utilisees: HashSet<String>,
    diagnostics: &'a mut Diagnostics,
}

impl Visiteur for ReferencesRegles<'_> {
    fn visiter_solution(&mut self, solution: &SolutionLitterale) {
        let mut dans_cette_solution: HashSet<&str> = HashSet::new();
        for r in &solution.regles {
            if !dans_cette_solution.insert(&r.nom) && !self.uniques.contains(r.nom.as_str()) {
                self.diagnostics.ajouter(
                    Diagnostic::avertissement(
                        "W0301",
                        format!("regle '{}' activee deux fois dans la meme solution", r.nom),
                    )
                    .etiquette(r.span, "doublon ignore sans effet sur le resultat"),
                );
            }
        }
        parcourir_solution(self, solution);
    }

    fn visiter_reference_regle(&mut self, nom: &Ident) {
        self.utilisees.insert(nom.nom.clone());
        if !self.definies.contains(nom.nom.as_str()) {
            let mut d = Diagnostic::erreur("E0302", format!("regle '{}' non definie", nom.nom))
                .etiquette(nom.span, "regle inconnue");
            d = match suggestion(&nom.nom, self.definies.iter().copied()) {
                Some(proche) => d.aide(format!("vouliez-vous dire '{proche}' ?")),
                None if self.definies.is_empty() => {
                    d.aide("definissez-la avec : let nom = replace ... by ... ;")
                }
                None => {
                    let mut noms: Vec<&str> = self.definies.iter().copied().collect();
                    noms.sort_unstable();
                    d.aide(format!("regles definies : {}", noms.join(", ")))
                }
            };
            self.diagnostics.ajouter(d);
        }
    }
}

impl Passe for ReglesDefinies {
    fn nom(&self) -> &'static str {
        "regles-definies"
    }

    fn analyser(&self, programme: &Programme, diagnostics: &mut Diagnostics) {
        let definies: HashSet<&str> = programme
            .definitions
            .iter()
            .map(|d| d.nom.nom.as_str())
            .collect();
        let uniques: HashSet<&str> = programme
            .definitions
            .iter()
            .filter(|d| d.regle.unique)
            .map(|d| d.nom.nom.as_str())
            .collect();
        let mut v = ReferencesRegles {
            definies: &definies,
            uniques: &uniques,
            utilisees: HashSet::new(),
            diagnostics,
        };
        v.visiter_solution(&programme.solution);
        let utilisees = v.utilisees;
        for d in &programme.definitions {
            if !utilisees.contains(&d.nom.nom) {
                diagnostics.ajouter(
                    Diagnostic::avertissement(
                        "W0302",
                        format!("regle '{}' definie mais jamais activee", d.nom.nom),
                    )
                    .etiquette(d.nom.span, "inutilisee"),
                );
            }
        }
    }
}

/// Variables des regles : liaison unique, utilisation de variables liees.
pub struct VariablesLiees;

fn variables_motif<'a>(
    motif: &'a Motif,
    sortie: &mut Vec<(&'a Ident, Option<&'a TypeAnnote>, bool)>,
    dans_nuplet: bool,
) {
    match motif {
        Motif::Variable { nom, type_annote } => {
            sortie.push((nom, type_annote.as_ref(), dans_nuplet))
        }
        Motif::NUplet { elements, .. } => {
            for m in elements {
                variables_motif(m, sortie, true);
            }
        }
    }
}

impl Passe for VariablesLiees {
    fn nom(&self) -> &'static str {
        "variables-liees"
    }

    fn analyser(&self, programme: &Programme, diagnostics: &mut Diagnostics) {
        for d in &programme.definitions {
            let regle = &d.regle;
            let mut liees: Vec<(&Ident, Option<&TypeAnnote>, bool)> = Vec::new();
            for m in &regle.reactifs {
                variables_motif(m, &mut liees, false);
            }
            let mut noms: HashMap<&str, Span> = HashMap::new();
            for (ident, _, _) in &liees {
                if let Some(premiere) = noms.get(ident.nom.as_str()) {
                    diagnostics.ajouter(
                        Diagnostic::erreur("E0303", format!("la variable '{}' est liee plusieurs fois", ident.nom))
                            .etiquette(ident.span, "seconde liaison")
                            .etiquette(*premiere, "premiere liaison")
                            .aide("chaque reactif doit avoir son propre nom ; comparez-les dans la condition"),
                    );
                } else {
                    noms.insert(&ident.nom, ident.span);
                }
            }
            let non_liee = |nom: &str, span: Span, diagnostics: &mut Diagnostics| {
                if !noms.contains_key(nom) {
                    let mut diag =
                        Diagnostic::erreur("E0304", format!("variable '{nom}' non liee"))
                            .etiquette(span, "absente des reactifs");
                    diag = match suggestion(nom, noms.keys().copied()) {
                        Some(proche) => diag.aide(format!("vouliez-vous dire '{proche}' ?")),
                        None => diag.aide("une variable utilisee dans une condition ou un produit doit apparaitre dans les reactifs"),
                    };
                    diagnostics.ajouter(diag);
                }
            };
            let mut expressions: Vec<&Expr> = Vec::new();
            for p in &regle.produits {
                match p {
                    Produit::Expression(e) => expressions.push(e),
                    Produit::Dissolution { cible, .. } => {
                        non_liee(&cible.nom, cible.span, diagnostics)
                    }
                    Produit::Injection { valeur, cible, .. } => {
                        expressions.push(valeur);
                        non_liee(&cible.nom, cible.span, diagnostics);
                    }
                }
            }
            expressions.extend(regle.condition.iter());
            for e in expressions {
                for v in CollecteurVariables::dans(e) {
                    if let GenreExpr::Variable(nom) = &v.genre {
                        non_liee(nom, v.span, diagnostics);
                    }
                }
            }
        }
    }
}

/// Types : conditions booleennes, operations coherentes, sous-solutions.
pub struct TypesCoherents;

impl TypesCoherents {
    fn environnement(regle: &Regle, diagnostics: &mut Diagnostics) -> EnvironnementTypes {
        let mut liees = Vec::new();
        for m in &regle.reactifs {
            variables_motif(m, &mut liees, false);
        }
        let mut env = EnvironnementTypes::new();
        for (ident, annotation, dans_nuplet) in liees {
            let t = match annotation.map(|a| a.genre) {
                None => TypeStatique::Valeur,
                Some(GenreType::Int) => TypeStatique::Entier,
                Some(GenreType::Bool) => TypeStatique::Booleen,
                Some(GenreType::Str) => TypeStatique::Chaine,
                Some(genre @ (GenreType::Sol | GenreType::Ref)) if dans_nuplet => {
                    diagnostics.ajouter(
                        Diagnostic::erreur(
                            "E0310",
                            "une sous-solution ne peut pas etre une composante de n-uplet",
                        )
                        .etiquette(
                            ident.span,
                            format!("motif '{}' dans un n-uplet", genre.nom()),
                        ),
                    );
                    TypeStatique::Inconnu
                }
                Some(GenreType::Sol) => TypeStatique::Solution,
                Some(GenreType::Ref) => TypeStatique::Reference,
            };
            env.entry(ident.nom.clone()).or_insert(t);
        }
        env
    }
}

impl Passe for TypesCoherents {
    fn nom(&self) -> &'static str {
        "types-coherents"
    }

    fn analyser(&self, programme: &Programme, diagnostics: &mut Diagnostics) {
        for d in &programme.definitions {
            let regle = &d.regle;
            let env = Self::environnement(regle, diagnostics);
            if let Some(c) = &regle.condition {
                let t = inferer(c, &env, diagnostics);
                if !matches!(
                    t,
                    TypeStatique::Booleen | TypeStatique::Valeur | TypeStatique::Inconnu
                ) {
                    diagnostics.ajouter(
                        Diagnostic::erreur(
                            "E0305",
                            format!("la condition doit etre booleenne, elle est de type {t}"),
                        )
                        .etiquette(c.span, "condition non booleenne")
                        .aide("exemple : if x >= y"),
                    );
                }
            }
            let mut usages_solutions: HashMap<String, Span> = HashMap::new();
            let mut usage_solution = |nom: &str, span: Span, diagnostics: &mut Diagnostics| {
                if let Some(premier) = usages_solutions.get(nom) {
                    diagnostics.ajouter(
                        Diagnostic::erreur(
                            "E0309",
                            format!("la sous-solution '{nom}' ne peut apparaitre qu'une fois dans les produits"),
                        )
                        .etiquette(span, "seconde utilisation")
                        .etiquette(*premier, "premiere utilisation")
                        .aide("une sous-solution liee ne peut etre ni dupliquee ni a la fois dissoute et conservee"),
                    );
                } else {
                    usages_solutions.insert(nom.to_string(), span);
                }
            };
            for p in &regle.produits {
                match p {
                    Produit::Expression(Expr {
                        genre: GenreExpr::Variable(nom),
                        span,
                    }) if env.get(nom) == Some(&TypeStatique::Solution) => {
                        usage_solution(nom, *span, diagnostics);
                    }
                    Produit::Expression(e) => {
                        inferer(e, &env, diagnostics);
                    }
                    Produit::Dissolution { cible, span }
                    | Produit::Injection { cible, span, .. } => {
                        if let Produit::Injection { valeur, .. } = p {
                            inferer(valeur, &env, diagnostics);
                        }
                        let dissolution = matches!(p, Produit::Dissolution { .. });
                        match env.get(&cible.nom) {
                            Some(TypeStatique::Solution) => {
                                usage_solution(&cible.nom, *span, diagnostics)
                            }
                            // Injection asynchrone : une reference n'est pas
                            // consommee et peut recevoir plusieurs injections.
                            Some(TypeStatique::Reference) if !dissolution => {}
                            Some(TypeStatique::Reference) => diagnostics.ajouter(
                                Diagnostic::erreur(
                                    "E0308",
                                    format!("'dissolve' attend une sous-solution liee, '{}' est une reference", cible.nom),
                                )
                                .etiquette(cible.span, "reference (s:ref)")
                                .aide(format!(
                                    "une reference ne sert qu'a inject(v, {0}) ; pour dissoudre, liez la sous-solution inerte : {0}:sol",
                                    cible.nom
                                )),
                            ),
                            Some(TypeStatique::Inconnu) | None => {}
                            Some(t) => {
                                let (operation, aide) = if dissolution {
                                    ("dissolve", format!("annotez le reactif : {}:sol", cible.nom))
                                } else {
                                    (
                                        "inject",
                                        format!(
                                            "annotez le reactif : {0}:sol (sous-solution inerte) ou {0}:ref (injection asynchrone)",
                                            cible.nom
                                        ),
                                    )
                                };
                                diagnostics.ajouter(
                                    Diagnostic::erreur(
                                        "E0308",
                                        format!("'{operation}' attend une sous-solution, '{}' est de type {t}", cible.nom),
                                    )
                                    .etiquette(cible.span, "pas une sous-solution")
                                    .aide(aide),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Regles qui ne consomment rien (aucun reactif, ou seulement des `s:ref`) :
/// toujours applicables, elles ne terminent que si elles sont a usage
/// unique (EF-D5). Une telle regle a usage multiple est signalee.
pub struct ReglesSansReactif;

impl Passe for ReglesSansReactif {
    fn nom(&self) -> &'static str {
        "regles-sans-reactif"
    }

    fn analyser(&self, programme: &Programme, diagnostics: &mut Diagnostics) {
        for d in &programme.definitions {
            if d.regle.unique || !d.regle.ne_consomme_rien() {
                continue;
            }
            let quoi = if d.regle.reactifs.is_empty() {
                "n'a aucun reactif"
            } else {
                "ne consomme aucun reactif (une reference s:ref n'est pas consommee)"
            };
            diagnostics.ajouter(
                Diagnostic::avertissement(
                    "W0303",
                    format!(
                        "la regle '{}' {quoi} : toujours applicable, elle s'appliquera sans fin",
                        d.nom.nom
                    ),
                )
                .etiquette(d.regle.span, "regle a usage multiple")
                .aide("ecrivez 'replace-one' pour qu'elle ne s'applique qu'une fois ; sinon seul un garde-fou (--max-etapes, --delai-max, --max-molecules) arretera l'execution"),
            );
        }
    }
}

/// Solutions litterales : elements constants, bornes d'intervalles entieres.
pub struct SolutionsConstantes;

struct VerificationSolutions<'a> {
    diagnostics: &'a mut Diagnostics,
}

impl VerificationSolutions<'_> {
    fn constante(&mut self, e: &Expr) -> TypeStatique {
        let variables = CollecteurVariables::dans(e);
        if variables.is_empty() {
            return inferer(e, &EnvironnementTypes::new(), self.diagnostics);
        }
        for v in variables {
            if let GenreExpr::Variable(nom) = &v.genre {
                self.diagnostics.ajouter(
                    Diagnostic::erreur("E0311", format!("variable '{nom}' dans une solution"))
                        .etiquette(v.span, "les solutions ne contiennent que des constantes")
                        .aide("les variables n'existent que dans les regles ; ecrivez une valeur (4, true, \"a\", (1, 2))"),
                );
            }
        }
        TypeStatique::Inconnu
    }
}

impl Visiteur for VerificationSolutions<'_> {
    fn visiter_element(&mut self, element: &Element) {
        match element {
            Element::Expression(e) => {
                self.constante(e);
            }
            Element::Intervalle { debut, fin, .. } => {
                for borne in [debut, fin] {
                    let t = self.constante(borne);
                    if !matches!(t, TypeStatique::Entier | TypeStatique::Inconnu) {
                        self.diagnostics.ajouter(
                            Diagnostic::erreur("E0312", format!("borne d'intervalle de type {t}"))
                                .etiquette(borne.span, "un entier est attendu"),
                        );
                    }
                }
            }
            Element::Solution(_) => parcourir_element(self, element),
        }
    }
}

impl Passe for SolutionsConstantes {
    fn nom(&self) -> &'static str {
        "solutions-constantes"
    }

    fn analyser(&self, programme: &Programme, diagnostics: &mut Diagnostics) {
        VerificationSolutions { diagnostics }.visiter_solution(&programme.solution);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_et_suggestions() {
        assert_eq!(distance("max", "max"), 0);
        assert_eq!(distance("mx", "max"), 1);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(
            suggestion("mxa", ["max", "crible"].into_iter()),
            Some("max")
        );
        assert_eq!(suggestion("zzz", ["max"].into_iter()), None);
    }
}
