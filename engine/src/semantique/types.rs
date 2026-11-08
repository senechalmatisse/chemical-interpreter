//! Typage statique leger des expressions (incoherences evidentes).
//!
//! Les variables non annotees ont le type `valeur` (toute valeur sauf une
//! sous-solution) : les operations qui les concernent sont verifiees a
//! l'execution. Seules les incoherences certaines sont signalees.

use crate::ast::{Expr, GenreExpr, OpBinaire, OpUnaire};
use crate::diagnostic::{Diagnostic, Diagnostics};
use std::collections::HashMap;
use std::fmt;

/// Type statique d'une expression ou d'une variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeStatique {
    /// Entier.
    Entier,
    /// Booleen.
    Booleen,
    /// Chaine.
    Chaine,
    /// N-uplet d'arite connue.
    NUplet(usize),
    /// Valeur de type inconnu (verifiee a l'execution).
    Valeur,
    /// Sous-solution.
    Solution,
    /// Reference a une sous-solution (`s:ref`), cible d'injection asynchrone.
    Reference,
    /// Erreur deja signalee (evite les cascades).
    Inconnu,
}

impl TypeStatique {
    fn est_connu(&self) -> bool {
        !matches!(self, TypeStatique::Valeur | TypeStatique::Inconnu)
    }
}

impl fmt::Display for TypeStatique {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeStatique::Entier => f.write_str("int"),
            TypeStatique::Booleen => f.write_str("bool"),
            TypeStatique::Chaine => f.write_str("str"),
            TypeStatique::NUplet(n) => write!(f, "n-uplet de {n}"),
            TypeStatique::Valeur => f.write_str("valeur"),
            TypeStatique::Solution => f.write_str("sol"),
            TypeStatique::Reference => f.write_str("ref"),
            TypeStatique::Inconnu => f.write_str("?"),
        }
    }
}

/// Environnement de typage : variable -> type.
pub type EnvironnementTypes = HashMap<String, TypeStatique>;

/// Infere le type d'une expression ; signale les incoherences evidentes.
pub fn inferer(
    expr: &Expr,
    env: &EnvironnementTypes,
    diagnostics: &mut Diagnostics,
) -> TypeStatique {
    use TypeStatique::*;
    match &expr.genre {
        GenreExpr::Entier(_) => Entier,
        GenreExpr::Booleen(_) => Booleen,
        GenreExpr::Chaine(_) => Chaine,
        GenreExpr::Variable(nom) => {
            match env.get(nom) {
                Some(Solution) => {
                    diagnostics.ajouter(
                    Diagnostic::erreur(
                        "E0307",
                        format!("la sous-solution '{nom}' ne peut pas etre utilisee dans une expression"),
                    )
                    .etiquette(expr.span, "sous-solution")
                    .aide(format!(
                        "une sous-solution s'utilise seule comme produit : {nom}, dissolve({nom}) ou inject(v, {nom})"
                    )),
                );
                    Inconnu
                }
                Some(Reference) => {
                    diagnostics.ajouter(
                    Diagnostic::erreur(
                        "E0307",
                        format!("la reference '{nom}' ne peut pas etre utilisee dans une expression"),
                    )
                    .etiquette(expr.span, "reference de sous-solution")
                    .aide(format!(
                        "une reference (s:ref) ne sert que de cible d'injection : inject(v, {nom})"
                    )),
                );
                    Inconnu
                }
                Some(t) => *t,
                None => Inconnu,
            }
        }
        GenreExpr::NUplet(elements) => {
            for e in elements {
                inferer(e, env, diagnostics);
            }
            NUplet(elements.len())
        }
        GenreExpr::Unaire { op, operande } => {
            let t = inferer(operande, env, diagnostics);
            let (attendu, resultat) = match op {
                OpUnaire::Moins => (Entier, Entier),
                OpUnaire::Non => (Booleen, Booleen),
            };
            if t.est_connu() && t != attendu {
                diagnostics.ajouter(
                    Diagnostic::erreur(
                        "E0306",
                        format!("operation '{}' impossible sur {t}", op.symbole()),
                    )
                    .etiquette(operande.span, format!("{attendu} attendu, {t} trouve")),
                );
                return Inconnu;
            }
            if t == Inconnu { Inconnu } else { resultat }
        }
        GenreExpr::Binaire { op, gauche, droite } => {
            let g = inferer(gauche, env, diagnostics);
            let d = inferer(droite, env, diagnostics);
            if g == Inconnu || d == Inconnu {
                return if op.est_comparaison() || matches!(op, OpBinaire::Et | OpBinaire::Ou) {
                    Booleen
                } else {
                    Inconnu
                };
            }
            match typer_binaire(*op, g, d) {
                Some(t) => t,
                None => {
                    diagnostics.ajouter(
                        Diagnostic::erreur(
                            "E0306",
                            format!("operation '{}' impossible entre {g} et {d}", op.symbole()),
                        )
                        .etiquette(expr.span, "types incompatibles")
                        .aide(aide_operateur(*op)),
                    );
                    Inconnu
                }
            }
        }
    }
}

fn aide_operateur(op: OpBinaire) -> &'static str {
    use OpBinaire::*;
    match op {
        Plus => "'+' additionne deux entiers ou concatene deux chaines",
        Moins | Fois | Division | Modulo => "cet operateur s'applique a deux entiers",
        Egal | Different => "les deux membres doivent etre du meme type",
        Inferieur | InferieurEgal | Superieur | SuperieurEgal => {
            "les comparaisons d'ordre portent sur deux entiers ou deux chaines"
        }
        Et | Ou => "les operateurs logiques portent sur des booleens",
    }
}

/// Type du resultat d'une operation binaire, `None` si impossible.
fn typer_binaire(op: OpBinaire, g: TypeStatique, d: TypeStatique) -> Option<TypeStatique> {
    use OpBinaire::*;
    use TypeStatique::*;
    let compatible = |a: TypeStatique, b: TypeStatique, admis: &[TypeStatique]| {
        let ok = |t: TypeStatique| t == Valeur || admis.contains(&t);
        ok(a) && ok(b) && (a == b || a == Valeur || b == Valeur)
    };
    match op {
        Plus => {
            if !compatible(g, d, &[Entier, Chaine]) {
                return None;
            }
            Some(match (g, d) {
                (Chaine, _) | (_, Chaine) => Chaine,
                (Entier, _) | (_, Entier) => Entier,
                _ => Valeur,
            })
        }
        Moins | Fois | Division | Modulo => compatible(g, d, &[Entier]).then_some(Entier),
        Egal | Different => (!(g.est_connu() && d.est_connu()) || g == d).then_some(Booleen),
        Inferieur | InferieurEgal | Superieur | SuperieurEgal => {
            compatible(g, d, &[Entier, Chaine]).then_some(Booleen)
        }
        Et | Ou => compatible(g, d, &[Booleen]).then_some(Booleen),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::analyser_expression;

    fn typer(texte: &str, env: &[(&str, TypeStatique)]) -> (TypeStatique, Vec<&'static str>) {
        let e = analyser_expression(texte).unwrap();
        let env: EnvironnementTypes = env.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let mut d = Diagnostics::new();
        let t = inferer(&e, &env, &mut d);
        (t, d.iter().map(|x| x.code).collect())
    }

    #[test]
    fn inference() {
        use TypeStatique::*;
        assert_eq!(
            typer("x >= y", &[("x", Valeur), ("y", Valeur)]),
            (Booleen, vec![])
        );
        assert_eq!(typer("x + 1", &[("x", Valeur)]), (Entier, vec![]));
        assert_eq!(typer("\"a\" + s", &[("s", Valeur)]), (Chaine, vec![]));
        assert_eq!(typer("(1, x)", &[("x", Entier)]), (NUplet(2), vec![]));
        assert_eq!(typer("1 == true", &[]).1, vec!["E0306"]);
        assert_eq!(typer("x + true", &[("x", Entier)]).1, vec!["E0306"]);
        assert_eq!(typer("not 3", &[]).1, vec!["E0306"]);
        assert_eq!(
            typer("x and y", &[("x", Entier), ("y", Booleen)]).1,
            vec!["E0306"]
        );
        assert_eq!(typer("s + 1", &[("s", Solution)]).1, vec!["E0307"]);
        // une seule erreur, pas de cascade
        assert_eq!(typer("(1 + true) * 2 == 3", &[]).1, vec!["E0306"]);
    }
}
