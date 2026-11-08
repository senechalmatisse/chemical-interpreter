//! Patron Visiteur sur l'AST.
//!
//! Chaque methode `visiter_*` a une implementation par defaut qui poursuit
//! le parcours (fonctions `parcourir_*`). Une analyse ne redefinit que les
//! noeuds qui l'interessent (principe ouvert/ferme : ajouter une analyse ne
//! modifie ni l'AST ni les analyses existantes).

use super::*;

/// Visiteur de l'AST (parcours en profondeur, ordre du source).
pub trait Visiteur {
    /// Programme complet.
    fn visiter_programme(&mut self, programme: &Programme) {
        parcourir_programme(self, programme);
    }

    /// Definition de regle.
    fn visiter_definition(&mut self, definition: &Definition) {
        parcourir_definition(self, definition);
    }

    /// Regle (reactifs, produits, condition).
    fn visiter_regle(&mut self, regle: &Regle) {
        parcourir_regle(self, regle);
    }

    /// Motif de reactif.
    fn visiter_motif(&mut self, motif: &Motif) {
        parcourir_motif(self, motif);
    }

    /// Produit de regle.
    fn visiter_produit(&mut self, produit: &Produit) {
        parcourir_produit(self, produit);
    }

    /// Solution litterale (y compris imbriquee).
    fn visiter_solution(&mut self, solution: &SolutionLitterale) {
        parcourir_solution(self, solution);
    }

    /// Element de solution.
    fn visiter_element(&mut self, element: &Element) {
        parcourir_element(self, element);
    }

    /// Reference a une regle dans une solution (`| nom`).
    fn visiter_reference_regle(&mut self, _nom: &Ident) {}

    /// Expression.
    fn visiter_expr(&mut self, expr: &Expr) {
        parcourir_expr(self, expr);
    }
}

/// Parcours par defaut d'un programme.
pub fn parcourir_programme<V: Visiteur + ?Sized>(v: &mut V, programme: &Programme) {
    for definition in &programme.definitions {
        v.visiter_definition(definition);
    }
    v.visiter_solution(&programme.solution);
}

/// Parcours par defaut d'une definition.
pub fn parcourir_definition<V: Visiteur + ?Sized>(v: &mut V, definition: &Definition) {
    v.visiter_regle(&definition.regle);
}

/// Parcours par defaut d'une regle.
pub fn parcourir_regle<V: Visiteur + ?Sized>(v: &mut V, regle: &Regle) {
    for motif in &regle.reactifs {
        v.visiter_motif(motif);
    }
    for produit in &regle.produits {
        v.visiter_produit(produit);
    }
    if let Some(condition) = &regle.condition {
        v.visiter_expr(condition);
    }
}

/// Parcours par defaut d'un motif.
pub fn parcourir_motif<V: Visiteur + ?Sized>(v: &mut V, motif: &Motif) {
    if let Motif::NUplet { elements, .. } = motif {
        for m in elements {
            v.visiter_motif(m);
        }
    }
}

/// Parcours par defaut d'un produit.
pub fn parcourir_produit<V: Visiteur + ?Sized>(v: &mut V, produit: &Produit) {
    match produit {
        Produit::Expression(e) => v.visiter_expr(e),
        Produit::Dissolution { .. } => {}
        Produit::Injection { valeur, .. } => v.visiter_expr(valeur),
    }
}

/// Parcours par defaut d'une solution.
pub fn parcourir_solution<V: Visiteur + ?Sized>(v: &mut V, solution: &SolutionLitterale) {
    for element in &solution.elements {
        v.visiter_element(element);
    }
    for regle in &solution.regles {
        v.visiter_reference_regle(regle);
    }
}

/// Parcours par defaut d'un element.
pub fn parcourir_element<V: Visiteur + ?Sized>(v: &mut V, element: &Element) {
    match element {
        Element::Expression(e) => v.visiter_expr(e),
        Element::Intervalle { debut, fin, .. } => {
            v.visiter_expr(debut);
            v.visiter_expr(fin);
        }
        Element::Solution(s) => v.visiter_solution(s),
    }
}

/// Parcours par defaut d'une expression.
pub fn parcourir_expr<V: Visiteur + ?Sized>(v: &mut V, expr: &Expr) {
    match &expr.genre {
        GenreExpr::NUplet(es) => {
            for e in es {
                v.visiter_expr(e);
            }
        }
        GenreExpr::Unaire { operande, .. } => v.visiter_expr(operande),
        GenreExpr::Binaire { gauche, droite, .. } => {
            v.visiter_expr(gauche);
            v.visiter_expr(droite);
        }
        _ => {}
    }
}

/// Visiteur utilitaire : collecte les variables utilisees dans des expressions.
#[derive(Debug, Default)]
pub struct CollecteurVariables<'a> {
    /// Variables rencontrees, dans l'ordre, avec leur expression.
    pub variables: Vec<&'a Expr>,
}

impl<'a> CollecteurVariables<'a> {
    /// Collecte les variables d'une expression.
    pub fn dans(expr: &'a Expr) -> Vec<&'a Expr> {
        let mut pile = vec![expr];
        let mut variables = Vec::new();
        while let Some(e) = pile.pop() {
            match &e.genre {
                GenreExpr::Variable(_) => variables.push(e),
                GenreExpr::NUplet(es) => pile.extend(es.iter().rev()),
                GenreExpr::Unaire { operande, .. } => pile.push(operande),
                GenreExpr::Binaire { gauche, droite, .. } => {
                    pile.push(droite);
                    pile.push(gauche);
                }
                _ => {}
            }
        }
        variables
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::analyser_programme;
    use crate::source::Source;

    /// Compte les noeuds visites : verifie que le parcours par defaut est complet.
    #[derive(Default)]
    struct Compteur {
        expressions: usize,
        motifs: usize,
        solutions: usize,
        references: usize,
    }

    impl Visiteur for Compteur {
        fn visiter_expr(&mut self, e: &Expr) {
            self.expressions += 1;
            parcourir_expr(self, e);
        }
        fn visiter_motif(&mut self, m: &Motif) {
            self.motifs += 1;
            parcourir_motif(self, m);
        }
        fn visiter_solution(&mut self, s: &SolutionLitterale) {
            self.solutions += 1;
            parcourir_solution(self, s);
        }
        fn visiter_reference_regle(&mut self, _: &Ident) {
            self.references += 1;
        }
    }

    #[test]
    fn parcours_complet() {
        let source = Source::new(
            "t",
            "let r = replace (a, b), s:sol by inject(a + 1, s) if a < b;\n{ 1..3, { 4 | r } | r }",
        );
        let programme = analyser_programme(&source).unwrap();
        let mut c = Compteur::default();
        c.visiter_programme(&programme);
        // a + 1 (3 noeuds) ; a < b (3 noeuds) ; 1, 3, 4 (3 noeuds)
        assert_eq!(c.expressions, 9);
        assert_eq!(c.motifs, 4);
        assert_eq!(c.solutions, 2);
        assert_eq!(c.references, 2);
    }
}
