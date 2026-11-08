//! Analyse semantique et compilation.
//!
//! L'[`Analyseur`] enchaine des [`Passe`]s independantes (noms uniques,
//! regles definies, variables liees, types, solutions constantes) et
//! accumule tous leurs diagnostics. Ajouter une verification revient a
//! ecrire une nouvelle passe, sans toucher aux autres (ouvert/ferme).

mod compilation;
mod passes;
pub mod types;

pub use compilation::{ProgrammeCompile, compiler, compiler_expression, compiler_regle};
pub use passes::{
    NomsUniques, Passe, ReglesDefinies, ReglesSansReactif, SolutionsConstantes, TypesCoherents,
    VariablesLiees, suggestion,
};

use crate::ast::Programme;
use crate::diagnostic::Diagnostics;

/// Enchainement de passes de verification.
pub struct Analyseur {
    passes: Vec<Box<dyn Passe>>,
}

impl Default for Analyseur {
    fn default() -> Self {
        Self::standard()
    }
}

impl Analyseur {
    /// Analyseur sans passe.
    pub fn vide() -> Self {
        Self { passes: Vec::new() }
    }

    /// Passes standard du langage.
    pub fn standard() -> Self {
        Self::vide()
            .avec(NomsUniques)
            .avec(ReglesDefinies)
            .avec(VariablesLiees)
            .avec(TypesCoherents)
            .avec(ReglesSansReactif)
            .avec(SolutionsConstantes)
    }

    /// Ajoute une passe en fin de chaine.
    pub fn avec(mut self, passe: impl Passe + 'static) -> Self {
        self.passes.push(Box::new(passe));
        self
    }

    /// Noms des passes, dans l'ordre.
    pub fn noms(&self) -> Vec<&'static str> {
        self.passes.iter().map(|p| p.nom()).collect()
    }

    /// Execute toutes les passes et retourne leurs diagnostics.
    pub fn analyser(&self, programme: &Programme) -> Diagnostics {
        let mut diagnostics = Diagnostics::new();
        for passe in &self.passes {
            passe.analyser(programme, &mut diagnostics);
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::analyser_programme;
    use crate::source::Source;

    fn codes(texte: &str) -> Vec<&'static str> {
        let p = analyser_programme(&Source::new("t", texte)).expect("syntaxe valide");
        Analyseur::standard()
            .analyser(&p)
            .iter()
            .map(|d| d.code)
            .collect()
    }

    #[test]
    fn programmes_valides() {
        assert!(codes("let max = replace x, y by x if x >= y; { 4, 17 | max }").is_empty());
        assert!(codes(
            "let max = replace x, y by x if x >= y; let vider = replace s:sol by dissolve(s);\n{ { 4 | max }, { 3 | max } | vider, max }"
        )
        .is_empty());
        assert!(codes("let envoyer = replace x:int, s:sol by inject(x, s); { 1, { 2 | envoyer } | envoyer }").is_empty());
        assert!(codes("let garder = replace s:sol, x by s; { { }, 1 | garder }").is_empty());
        assert!(codes("let t = replace (i, a), (j, b) by (i, b), (j, a) if i < j and a > b; { (0, 1) | t }").is_empty());
    }

    #[test]
    fn erreurs_semantiques() {
        assert_eq!(
            codes("let r = replace x by x; let r = replace y by y; { | r }"),
            vec!["E0301"]
        );
        assert_eq!(
            codes("let max = replace x, y by x; { 1 | mx }"),
            vec!["E0302", "W0302"]
        );
        assert_eq!(codes("let r = replace x, x by x; { | r }"), vec!["E0303"]);
        assert_eq!(codes("let r = replace x by y; { | r }"), vec!["E0304"]);
        assert_eq!(
            codes("let r = replace x by x if z > 0; { | r }"),
            vec!["E0304"]
        );
        assert_eq!(
            codes("let r = replace x by dissolve(s); { | r }"),
            vec!["E0304"]
        );
        assert_eq!(
            codes("let r = replace x:int by x if x + 1; { | r }"),
            vec!["E0305"]
        );
        assert_eq!(
            codes("let r = replace x:int, y:bool by x + y; { | r }"),
            vec!["E0306"]
        );
        assert_eq!(
            codes("let r = replace s:sol by s + 1; { | r }"),
            vec!["E0307"]
        );
        assert_eq!(
            codes("let r = replace x:int by dissolve(x); { | r }"),
            vec!["E0308"]
        );
        assert_eq!(
            codes("let r = replace s:sol by s, dissolve(s); { | r }"),
            vec!["E0309"]
        );
        assert_eq!(
            codes("let r = replace (s:sol, x) by x; { | r }"),
            vec!["E0310"]
        );
        assert_eq!(codes("let r = replace x by x; { y | r }"), vec!["E0311"]);
        assert_eq!(
            codes("let r = replace x by x; { 1..true | r }"),
            vec!["E0312"]
        );
    }

    #[test]
    fn avertissements() {
        assert_eq!(codes("let r = replace x by x; { 1 | r, r }"), vec!["W0301"]);
        assert_eq!(codes("let r = replace x by x; { 1 }"), vec!["W0302"]);
    }

    #[test]
    fn usage_unique_sans_reactif_et_references() {
        // valides
        assert!(codes("let a = replace-one by 1, 2; { | a }").is_empty());
        assert!(codes("let a = replace-one x by x + 1; { 5 | a, a }").is_empty());
        assert!(
            codes("let t = replace x:int, s:ref by inject(x, s), inject(x + 1, s); { 1, { } | t }")
                .is_empty()
        );
        assert!(
            codes(
                "let a = replace-one s:ref, t:ref by inject(1, s), inject(2, t); { { }, { } | a }"
            )
            .is_empty()
        );
        assert!(
            codes("let m = replace s:sol, t:ref by dissolve(s), inject(0, t); { { }, { } | m }")
                .is_empty()
        );
        // regle a usage multiple qui ne consomme rien
        assert_eq!(codes("let a = replace by 1; { | a }"), vec!["W0303"]);
        assert_eq!(
            codes("let a = replace s:ref by inject(1, s); { { } | a }"),
            vec!["W0303"]
        );
        // une reference n'est ni une valeur, ni dissoluble, ni composante de n-uplet
        assert_eq!(
            codes("let r = replace x, s:ref by x if s == 1; { | r }"),
            vec!["E0307"]
        );
        assert_eq!(
            codes("let r = replace x, s:ref by s; { | r }"),
            vec!["E0307"]
        );
        assert_eq!(
            codes("let r = replace x, s:ref by dissolve(s); { | r }"),
            vec!["E0308"]
        );
        assert_eq!(
            codes("let r = replace (x, s:ref) by x; { | r }"),
            vec!["E0310"]
        );
        assert_eq!(
            codes("let r = replace x:int, y by inject(x, y); { | r }"),
            vec!["E0308"]
        );
        assert_eq!(
            codes("let r = replace x by inject(x, s); { | r }"),
            vec!["E0304"]
        );
        assert_eq!(
            codes("let r = replace x:ref by x; { | r }"),
            vec!["E0307", "W0303"]
        );
    }

    #[test]
    fn chaine_extensible() {
        struct Interdire1;
        impl Passe for Interdire1 {
            fn nom(&self) -> &'static str {
                "test"
            }
            fn analyser(&self, _: &Programme, d: &mut Diagnostics) {
                d.ajouter(crate::diagnostic::Diagnostic::avertissement(
                    "W9999", "test",
                ));
            }
        }
        let a = Analyseur::standard().avec(Interdire1);
        assert_eq!(a.noms().last(), Some(&"test"));
        let p = analyser_programme(&Source::new("t", "{ }")).unwrap();
        assert_eq!(a.analyser(&p).len(), 1);
    }

    #[test]
    fn compilation_et_erreurs_de_chargement() {
        let p = analyser_programme(&Source::new(
            "t",
            "let r = replace x by x; { 1..3, 2 * 5, { 7 | r } | r }",
        ))
        .unwrap();
        let c = compiler(&p, 100).unwrap();
        assert_eq!(c.solution.to_string(), "{ 1, 2, 3, 10, { 7 | r } | r }");
        assert_eq!(c.regles["r"].arite(), 1);
        assert!(!c.regles["r"].unique);
        let p = analyser_programme(&Source::new(
            "t",
            "let e = replace-one s:sol, x, t:ref by inject(x, t), inject(1, s), x if x > 0; { 1, { }, { } | e }",
        ))
        .unwrap();
        let c = compiler(&p, 100).unwrap();
        let e = &c.regles["e"];
        assert!(e.unique);
        assert_eq!((e.arite(), e.references), (2, 1));
        assert_eq!(
            e.produits[..2],
            [
                crate::moteur::ProduitCompile::Envoi {
                    valeur: crate::evaluateur::ExprCompilee::Variable(0),
                    reference: 0,
                },
                crate::moteur::ProduitCompile::Injection {
                    valeur: crate::evaluateur::ExprCompilee::Constante(
                        crate::evaluateur::Valeur::Entier(1)
                    ),
                    reactif: 0,
                },
            ]
        );
        let code = |texte: &str, max: u64| {
            let p = analyser_programme(&Source::new("t", texte)).unwrap();
            compiler(&p, max)
                .unwrap_err()
                .iter()
                .map(|d| d.code)
                .collect::<Vec<_>>()
        };
        assert_eq!(code("{ 1 / 0 }", 10), vec!["E0313"]);
        assert_eq!(code("{ 5..1 }", 10), vec!["E0314"]);
        assert_eq!(code("{ 1..100 }", 10), vec!["E0315"]);
    }
}
