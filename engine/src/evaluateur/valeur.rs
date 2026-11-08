//! Valeurs portees par les molecules.

use crate::ast::afficheur::ecrire_chaine;
use std::fmt;
use std::sync::Arc;

/// Valeur d'une molecule (les sous-solutions ne sont pas des valeurs).
///
/// L'ordre total derive (entiers < booleens < chaines < n-uplets, puis
/// contenu) sert a l'affichage canonique des solutions. Le clonage est peu
/// couteux : chaines et n-uplets sont partages (`Arc`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Valeur {
    /// Entier relatif 64 bits.
    Entier(i64),
    /// Booleen.
    Booleen(bool),
    /// Chaine de caracteres.
    Chaine(Arc<str>),
    /// N-uplet de valeurs.
    NUplet(Arc<[Valeur]>),
}

impl Valeur {
    /// Construit une chaine.
    pub fn chaine(s: &str) -> Self {
        Valeur::Chaine(Arc::from(s))
    }

    /// Construit un n-uplet.
    pub fn nuplet(elements: Vec<Valeur>) -> Self {
        Valeur::NUplet(Arc::from(elements))
    }

    /// Nom du type dans le langage.
    pub fn nom_type(&self) -> &'static str {
        match self {
            Valeur::Entier(_) => "int",
            Valeur::Booleen(_) => "bool",
            Valeur::Chaine(_) => "str",
            Valeur::NUplet(_) => "n-uplet",
        }
    }

    /// Entier contenu, s'il s'agit d'un entier.
    pub fn comme_entier(&self) -> Option<i64> {
        match self {
            Valeur::Entier(n) => Some(*n),
            _ => None,
        }
    }
}

impl From<i64> for Valeur {
    fn from(n: i64) -> Self {
        Valeur::Entier(n)
    }
}

impl From<bool> for Valeur {
    fn from(b: bool) -> Self {
        Valeur::Booleen(b)
    }
}

impl fmt::Display for Valeur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Valeur::Entier(n) => write!(f, "{n}"),
            Valeur::Booleen(b) => write!(f, "{b}"),
            Valeur::Chaine(s) => ecrire_chaine(f, s),
            Valeur::NUplet(elements) => {
                f.write_str("(")?;
                for (i, e) in elements.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{e}")?;
                }
                f.write_str(")")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affichage_et_ordre() {
        let t = Valeur::nuplet(vec![Valeur::Entier(1), Valeur::chaine("a\"b")]);
        assert_eq!(t.to_string(), "(1, \"a\\\"b\")");
        let mut v = vec![
            Valeur::chaine("b"),
            t.clone(),
            Valeur::Booleen(true),
            Valeur::Entier(3),
            Valeur::Entier(-2),
        ];
        v.sort();
        assert_eq!(
            v,
            vec![
                Valeur::Entier(-2),
                Valeur::Entier(3),
                Valeur::Booleen(true),
                Valeur::chaine("b"),
                t
            ]
        );
    }
}
