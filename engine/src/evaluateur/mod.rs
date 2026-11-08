//! Evaluateur d'expressions.
//!
//! Les expressions de l'AST sont compilees en [`ExprCompilee`] : les
//! variables y sont resolues en indices d'emplacements, ce qui evite toute
//! recherche par nom pendant l'execution. Chaque variante sait s'evaluer
//! (`evaluer`) dans un environnement `&[Valeur]`. Aucune evaluation dynamique
//! du langage hote n'est utilisee.
//!
//! Semantique arithmetique (documentee dans docs/langage.md) :
//! entiers 64 bits avec controle de depassement ; `/` et `mod` euclidiens
//! (`mod` toujours positif ou nul) ; division par zero = erreur d'execution ;
//! `and`/`or` paresseux ; `==`/`!=` definis entre valeurs de tous types ;
//! `< <= > >=` entre entiers ou entre chaines ; `+` additionne les entiers
//! et concatene les chaines.

mod valeur;

pub use valeur::Valeur;

use crate::ast::{OpBinaire, OpUnaire};
use crate::source::Span;
use std::cmp::Ordering;
use std::fmt;

/// Expression compilee, prete a etre evaluee.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprCompilee {
    /// Valeur constante.
    Constante(Valeur),
    /// Lecture d'un emplacement de l'environnement.
    Variable(usize),
    /// Construction d'un n-uplet.
    NUplet(Vec<ExprCompilee>),
    /// Operation unaire.
    Unaire {
        /// Operateur.
        op: OpUnaire,
        /// Operande.
        operande: Box<ExprCompilee>,
        /// Position dans le source (pour les erreurs).
        span: Span,
    },
    /// Operation binaire.
    Binaire {
        /// Operateur.
        op: OpBinaire,
        /// Operande gauche.
        gauche: Box<ExprCompilee>,
        /// Operande droite.
        droite: Box<ExprCompilee>,
        /// Position dans le source (pour les erreurs).
        span: Span,
    },
}

/// Nature d'une erreur d'evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenreErreurEvaluation {
    /// Division ou modulo par zero.
    DivisionParZero,
    /// Depassement de la capacite des entiers 64 bits.
    Depassement,
    /// Operation appliquee a des valeurs du mauvais type.
    TypeInvalide(String),
}

/// Erreur survenue pendant l'evaluation d'une expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErreurEvaluation {
    /// Nature de l'erreur.
    pub genre: GenreErreurEvaluation,
    /// Operation fautive dans le source.
    pub span: Span,
}

impl ErreurEvaluation {
    /// Code stable du diagnostic associe.
    pub fn code(&self) -> &'static str {
        match self.genre {
            GenreErreurEvaluation::DivisionParZero => "E0401",
            GenreErreurEvaluation::Depassement => "E0402",
            GenreErreurEvaluation::TypeInvalide(_) => "E0403",
        }
    }

    fn type_invalide(span: Span, message: String) -> Self {
        Self {
            genre: GenreErreurEvaluation::TypeInvalide(message),
            span,
        }
    }

    /// Erreur pour une condition qui n'est pas booleenne.
    pub fn condition_non_booleenne(valeur: &Valeur, span: Span) -> Self {
        Self::type_invalide(
            span,
            format!(
                "la condition doit etre booleenne, elle vaut {valeur} ({})",
                valeur.nom_type()
            ),
        )
    }
}

impl fmt::Display for ErreurEvaluation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.genre {
            GenreErreurEvaluation::DivisionParZero => f.write_str("division par zero"),
            GenreErreurEvaluation::Depassement => {
                f.write_str("depassement de capacite des entiers 64 bits")
            }
            GenreErreurEvaluation::TypeInvalide(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ErreurEvaluation {}

impl ExprCompilee {
    /// Position de l'expression quand elle est connue.
    pub fn span(&self) -> Span {
        match self {
            ExprCompilee::Unaire { span, .. } | ExprCompilee::Binaire { span, .. } => *span,
            _ => Span::default(),
        }
    }

    /// Evalue l'expression dans l'environnement donne.
    pub fn evaluer(&self, env: &[Valeur]) -> Result<Valeur, ErreurEvaluation> {
        match self {
            ExprCompilee::Constante(v) => Ok(v.clone()),
            ExprCompilee::Variable(i) => Ok(env[*i].clone()),
            ExprCompilee::NUplet(elements) => {
                let valeurs = elements
                    .iter()
                    .map(|e| e.evaluer(env))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Valeur::nuplet(valeurs))
            }
            ExprCompilee::Unaire { op, operande, span } => {
                let v = operande.evaluer(env)?;
                match (op, v) {
                    (OpUnaire::Moins, Valeur::Entier(n)) => {
                        n.checked_neg().map(Valeur::Entier).ok_or(ErreurEvaluation {
                            genre: GenreErreurEvaluation::Depassement,
                            span: *span,
                        })
                    }
                    (OpUnaire::Non, Valeur::Booleen(b)) => Ok(Valeur::Booleen(!b)),
                    (op, v) => Err(ErreurEvaluation::type_invalide(
                        *span,
                        format!(
                            "operation '{}' impossible sur {v} ({})",
                            op.symbole(),
                            v.nom_type()
                        ),
                    )),
                }
            }
            ExprCompilee::Binaire {
                op: op @ (OpBinaire::Et | OpBinaire::Ou),
                gauche,
                droite,
                span,
            } => {
                let court_circuit = *op == OpBinaire::Ou;
                match gauche.evaluer(env)? {
                    Valeur::Booleen(b) if b == court_circuit => Ok(Valeur::Booleen(b)),
                    Valeur::Booleen(_) => match droite.evaluer(env)? {
                        Valeur::Booleen(b) => Ok(Valeur::Booleen(b)),
                        v => Err(Self::erreur_logique(*op, &v, *span)),
                    },
                    v => Err(Self::erreur_logique(*op, &v, *span)),
                }
            }
            ExprCompilee::Binaire {
                op,
                gauche,
                droite,
                span,
            } => {
                let g = gauche.evaluer(env)?;
                let d = droite.evaluer(env)?;
                appliquer_binaire(*op, &g, &d, *span)
            }
        }
    }

    fn erreur_logique(op: OpBinaire, v: &Valeur, span: Span) -> ErreurEvaluation {
        ErreurEvaluation::type_invalide(
            span,
            format!(
                "operation '{}' impossible : {v} ({}) n'est pas un booleen",
                op.symbole(),
                v.nom_type()
            ),
        )
    }
}

/// Applique un operateur binaire non paresseux a deux valeurs.
pub fn appliquer_binaire(
    op: OpBinaire,
    g: &Valeur,
    d: &Valeur,
    span: Span,
) -> Result<Valeur, ErreurEvaluation> {
    use OpBinaire::*;
    use Valeur::*;
    let depassement = || ErreurEvaluation {
        genre: GenreErreurEvaluation::Depassement,
        span,
    };
    let division_par_zero = || ErreurEvaluation {
        genre: GenreErreurEvaluation::DivisionParZero,
        span,
    };
    let comparer = |ordre: Ordering| {
        Ok(Booleen(match op {
            Inferieur => ordre == Ordering::Less,
            InferieurEgal => ordre != Ordering::Greater,
            Superieur => ordre == Ordering::Greater,
            _ => ordre != Ordering::Less,
        }))
    };
    match (op, g, d) {
        (Plus, Entier(a), Entier(b)) => a.checked_add(*b).map(Entier).ok_or_else(depassement),
        (Plus, Chaine(a), Chaine(b)) => Ok(Valeur::chaine(&format!("{a}{b}"))),
        (Moins, Entier(a), Entier(b)) => a.checked_sub(*b).map(Entier).ok_or_else(depassement),
        (Fois, Entier(a), Entier(b)) => a.checked_mul(*b).map(Entier).ok_or_else(depassement),
        (Division | Modulo, Entier(_), Entier(0)) => Err(division_par_zero()),
        (Division, Entier(a), Entier(b)) => {
            a.checked_div_euclid(*b).map(Entier).ok_or_else(depassement)
        }
        (Modulo, Entier(a), Entier(b)) => {
            a.checked_rem_euclid(*b).map(Entier).ok_or_else(depassement)
        }
        (Egal, _, _) => Ok(Booleen(g == d)),
        (Different, _, _) => Ok(Booleen(g != d)),
        (Inferieur | InferieurEgal | Superieur | SuperieurEgal, Entier(a), Entier(b)) => {
            comparer(a.cmp(b))
        }
        (Inferieur | InferieurEgal | Superieur | SuperieurEgal, Chaine(a), Chaine(b)) => {
            comparer(a.cmp(b))
        }
        _ => Err(ErreurEvaluation::type_invalide(
            span,
            format!(
                "operation '{}' impossible entre {g} ({}) et {d} ({})",
                op.symbole(),
                g.nom_type(),
                d.nom_type()
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(n: i64) -> Box<ExprCompilee> {
        Box::new(ExprCompilee::Constante(Valeur::Entier(n)))
    }

    fn bin(op: OpBinaire, g: Box<ExprCompilee>, d: Box<ExprCompilee>) -> ExprCompilee {
        ExprCompilee::Binaire {
            op,
            gauche: g,
            droite: d,
            span: Span::new(1, 2),
        }
    }

    #[test]
    fn arithmetique() {
        let env = [];
        assert_eq!(
            bin(OpBinaire::Plus, c(2), c(3)).evaluer(&env),
            Ok(Valeur::Entier(5))
        );
        assert_eq!(
            bin(OpBinaire::Modulo, c(-7), c(3)).evaluer(&env),
            Ok(Valeur::Entier(2))
        );
        assert_eq!(
            bin(OpBinaire::Division, c(-7), c(3)).evaluer(&env),
            Ok(Valeur::Entier(-3))
        );
        assert_eq!(
            bin(OpBinaire::Division, c(7), c(2)).evaluer(&env),
            Ok(Valeur::Entier(3))
        );
        let e = bin(OpBinaire::Division, c(1), c(0))
            .evaluer(&env)
            .unwrap_err();
        assert_eq!(e.genre, GenreErreurEvaluation::DivisionParZero);
        assert_eq!(e.code(), "E0401");
        let e = bin(OpBinaire::Fois, c(i64::MAX), c(2))
            .evaluer(&env)
            .unwrap_err();
        assert_eq!(e.genre, GenreErreurEvaluation::Depassement);
    }

    #[test]
    fn variables_et_comparaisons() {
        let env = [Valeur::Entier(4), Valeur::Entier(9)];
        let e = bin(
            OpBinaire::SuperieurEgal,
            Box::new(ExprCompilee::Variable(1)),
            Box::new(ExprCompilee::Variable(0)),
        );
        assert_eq!(e.evaluer(&env), Ok(Valeur::Booleen(true)));
        let chaines = bin(
            OpBinaire::Inferieur,
            Box::new(ExprCompilee::Constante(Valeur::chaine("abc"))),
            Box::new(ExprCompilee::Constante(Valeur::chaine("abd"))),
        );
        assert_eq!(chaines.evaluer(&[]), Ok(Valeur::Booleen(true)));
        let egal_types_differents = bin(
            OpBinaire::Egal,
            c(1),
            Box::new(ExprCompilee::Constante(Valeur::Booleen(true))),
        );
        assert_eq!(
            egal_types_differents.evaluer(&[]),
            Ok(Valeur::Booleen(false))
        );
    }

    #[test]
    fn logique_paresseuse_et_types() {
        let faux = Box::new(ExprCompilee::Constante(Valeur::Booleen(false)));
        let division = Box::new(bin(OpBinaire::Division, c(1), c(0)));
        // false and (1/0) : la partie droite n'est pas evaluee
        assert_eq!(
            bin(OpBinaire::Et, faux.clone(), division.clone()).evaluer(&[]),
            Ok(Valeur::Booleen(false))
        );
        assert!(bin(OpBinaire::Ou, faux, division).evaluer(&[]).is_err());
        let e = bin(
            OpBinaire::Plus,
            c(1),
            Box::new(ExprCompilee::Constante(Valeur::Booleen(true))),
        )
        .evaluer(&[])
        .unwrap_err();
        assert_eq!(e.code(), "E0403");
        assert!(
            e.to_string()
                .contains("impossible entre 1 (int) et true (bool)")
        );
    }

    #[test]
    fn concatenation_et_nuplets() {
        let e = bin(
            OpBinaire::Plus,
            Box::new(ExprCompilee::Constante(Valeur::chaine("ab"))),
            Box::new(ExprCompilee::Constante(Valeur::chaine("cd"))),
        );
        assert_eq!(e.evaluer(&[]), Ok(Valeur::chaine("abcd")));
        let t = ExprCompilee::NUplet(vec![ExprCompilee::Variable(0), *c(2)]);
        assert_eq!(
            t.evaluer(&[Valeur::Entier(1)]),
            Ok(Valeur::nuplet(vec![Valeur::Entier(1), Valeur::Entier(2)]))
        );
    }
}
