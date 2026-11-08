//! Afficheur AST -> texte.
//!
//! Le texte produit est un programme valide, avec le minimum de parentheses
//! compatible avec les priorites : `analyser(afficher(ast)) == ast` (aux
//! positions pres). C'est aussi le format de la commande `chem fmt`.

use super::*;
use std::fmt::{self, Display, Formatter, Write};

impl Display for Programme {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for definition in &self.definitions {
            writeln!(f, "{definition}")?;
        }
        if !self.definitions.is_empty() {
            writeln!(f)?;
        }
        writeln!(f, "{}", self.solution)
    }
}

impl Display for Definition {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "let {} = {};", self.nom.nom, self.regle)
    }
}

impl Display for Regle {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(if self.unique {
            "replace-one"
        } else {
            "replace"
        })?;
        if !self.reactifs.is_empty() {
            f.write_char(' ')?;
            ecrire_liste(f, &self.reactifs)?;
        }
        f.write_str(" by")?;
        if !self.produits.is_empty() {
            f.write_char(' ')?;
            ecrire_liste(f, &self.produits)?;
        }
        if let Some(condition) = &self.condition {
            write!(f, " if {condition}")?;
        }
        Ok(())
    }
}

impl Display for Motif {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Motif::Variable { nom, type_annote } => {
                f.write_str(&nom.nom)?;
                if let Some(t) = type_annote {
                    write!(f, ":{}", t.genre.nom())?;
                }
                Ok(())
            }
            Motif::NUplet { elements, .. } => {
                f.write_char('(')?;
                ecrire_liste(f, elements)?;
                f.write_char(')')
            }
        }
    }
}

impl Display for Produit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Produit::Expression(e) => write!(f, "{e}"),
            Produit::Dissolution { cible, .. } => write!(f, "dissolve({})", cible.nom),
            Produit::Injection { valeur, cible, .. } => {
                write!(f, "inject({valeur}, {})", cible.nom)
            }
        }
    }
}

impl Display for SolutionLitterale {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_char('{')?;
        if !self.elements.is_empty() {
            f.write_char(' ')?;
            ecrire_liste(f, &self.elements)?;
        }
        if !self.regles.is_empty() {
            f.write_str(" | ")?;
            let noms: Vec<&str> = self.regles.iter().map(|r| r.nom.as_str()).collect();
            f.write_str(&noms.join(", "))?;
        }
        f.write_str(" }")
    }
}

impl Display for Element {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Element::Expression(e) => write!(f, "{e}"),
            Element::Intervalle { debut, fin, .. } => write!(f, "{debut}..{fin}"),
            Element::Solution(s) => write!(f, "{s}"),
        }
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        ecrire_expr(f, self, 0)
    }
}

fn ecrire_liste<T: Display>(f: &mut Formatter<'_>, elements: &[T]) -> fmt::Result {
    for (i, e) in elements.iter().enumerate() {
        if i > 0 {
            f.write_str(", ")?;
        }
        write!(f, "{e}")?;
    }
    Ok(())
}

/// Ecrit une chaine avec ses sequences d'echappement.
pub fn ecrire_chaine(f: &mut impl Write, s: &str) -> fmt::Result {
    f.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\t' => f.write_str("\\t")?,
            '\r' => f.write_str("\\r")?,
            autre => f.write_char(autre)?,
        }
    }
    f.write_char('"')
}

/// Ecrit `expr` ; des parentheses sont ajoutees si sa priorite est
/// strictement inferieure a `minimum`.
fn ecrire_expr(f: &mut Formatter<'_>, expr: &Expr, minimum: u8) -> fmt::Result {
    let parentheses = expr.priorite() < minimum;
    if parentheses {
        f.write_char('(')?;
    }
    match &expr.genre {
        GenreExpr::Entier(n) => write!(f, "{n}")?,
        GenreExpr::Booleen(b) => write!(f, "{b}")?,
        GenreExpr::Chaine(s) => ecrire_chaine(f, s)?,
        GenreExpr::Variable(v) => f.write_str(v)?,
        GenreExpr::NUplet(elements) => {
            f.write_char('(')?;
            for (i, e) in elements.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                ecrire_expr(f, e, 0)?;
            }
            f.write_char(')')?;
        }
        GenreExpr::Unaire {
            op: OpUnaire::Moins,
            operande,
        } => {
            f.write_char('-')?;
            if matches!(
                operande.genre,
                GenreExpr::Unaire {
                    op: OpUnaire::Moins,
                    ..
                }
            ) {
                f.write_char(' ')?;
            }
            ecrire_expr(f, operande, priorite::UNAIRE)?;
        }
        GenreExpr::Unaire {
            op: OpUnaire::Non,
            operande,
        } => {
            f.write_str("not ")?;
            ecrire_expr(f, operande, priorite::NON)?;
        }
        GenreExpr::Binaire { op, gauche, droite } => {
            let p = op.priorite();
            // Associativite a gauche ; comparaisons non associatives.
            let minimum_gauche = if op.est_comparaison() { p + 1 } else { p };
            ecrire_expr(f, gauche, minimum_gauche)?;
            write!(f, " {} ", op.symbole())?;
            ecrire_expr(f, droite, p + 1)?;
        }
    }
    if parentheses {
        f.write_char(')')?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::ast::SansPositions;
    use crate::parser::{analyser_expression, analyser_programme};
    use crate::source::Source;

    fn aller_retour_expr(texte: &str) -> String {
        let e = analyser_expression(texte).unwrap();
        let affiche = e.to_string();
        let relu = analyser_expression(&affiche).unwrap();
        assert_eq!(
            e.sans_positions(),
            relu.sans_positions(),
            "{texte} -> {affiche}"
        );
        affiche
    }

    #[test]
    fn parentheses_minimales() {
        assert_eq!(aller_retour_expr("(a + b) * c"), "(a + b) * c");
        assert_eq!(aller_retour_expr("a + (b * c)"), "a + b * c");
        assert_eq!(aller_retour_expr("a - (b - c)"), "a - (b - c)");
        assert_eq!(aller_retour_expr("(a - b) - c"), "a - b - c");
        assert_eq!(aller_retour_expr("not (a and b)"), "not (a and b)");
        assert_eq!(aller_retour_expr("(not a) and b"), "not a and b");
        assert_eq!(aller_retour_expr("not a == b"), "not a == b");
        assert_eq!(aller_retour_expr("(a == b) == c"), "(a == b) == c");
        assert_eq!(aller_retour_expr("-(a + 1)"), "-(a + 1)");
        assert_eq!(aller_retour_expr("- -a"), "- -a");
        assert_eq!(aller_retour_expr("a - -1"), "a - -1");
        assert_eq!(aller_retour_expr("(not a) + 1"), "(not a) + 1");
        assert_eq!(aller_retour_expr("y mod x == 0"), "y mod x == 0");
        assert_eq!(
            aller_retour_expr("((1, \"a\\n\"), true)"),
            "((1, \"a\\n\"), true)"
        );
        assert_eq!(
            aller_retour_expr("i < j and a > b or not c"),
            "i < j and a > b or not c"
        );
    }

    #[test]
    fn programme_complet() {
        let texte = "let max = replace x, y by x if x >= y;\nlet vider = replace s:sol by dissolve(s);\nlet envoyer = replace x:int, s:sol by inject(x, s);\nlet rien = replace (i, a), z by;\n\n{ 1..3, -4, { 1, 2 | max }, { | max } | vider, max }\n";
        let programme = analyser_programme(&Source::new("t", texte)).unwrap();
        assert_eq!(programme.to_string(), texte);
    }

    #[test]
    fn usage_unique_sans_reactif_et_references() {
        let texte = "let amorcer = replace-one by 10, 20;\nlet bouclage = replace by if false;\nlet une_fois = replace-one x by x + 1;\nlet envoyer = replace x:int, s:ref by inject(x, s);\nlet alimenter = replace-one s:ref, t:ref by inject(1, s), inject(2, t);\n\n{ { | une_fois } | amorcer, bouclage, envoyer, alimenter }\n";
        let programme = analyser_programme(&Source::new("t", texte)).unwrap();
        assert_eq!(programme.to_string(), texte);
        let relu = analyser_programme(&Source::new("t", programme.to_string())).unwrap();
        assert_eq!(relu.sans_positions(), programme.sans_positions());
    }
}
