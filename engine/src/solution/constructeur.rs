//! Builder pour assembler une solution.
//!
//! Utilise par la compilation du programme et par les tests. L'identifiant
//! de la solution est reserve a la creation du monteur : en construisant le
//! parent avant ses enfants, la numerotation est prefixe (`s0` = racine).

use super::{BoiteAuxLettres, Element, GenerateurIds, Molecule, Solution};
use crate::evaluateur::Valeur;
use crate::moteur::RegleCompilee;
use std::sync::Arc;

/// Monteur de [`Solution`].
pub struct ConstructeurSolution<'a> {
    ids: &'a GenerateurIds,
    solution: Solution,
}

impl<'a> ConstructeurSolution<'a> {
    /// Commence une solution (son identifiant est alloue immediatement).
    pub fn nouvelle(ids: &'a GenerateurIds) -> Self {
        Self {
            ids,
            solution: Solution {
                id: ids.solution(),
                regles: Vec::new(),
                elements: Vec::new(),
                inerte: false,
                boite: Arc::new(BoiteAuxLettres::new()),
            },
        }
    }

    /// Generateur d'identifiants utilise.
    pub fn ids(&self) -> &'a GenerateurIds {
        self.ids
    }

    /// Active une regle.
    pub fn regle(mut self, regle: Arc<RegleCompilee>) -> Self {
        self.solution.regles.push(regle);
        self
    }

    /// Ajoute une molecule.
    pub fn valeur(mut self, valeur: Valeur) -> Self {
        self.ajouter_valeur(valeur);
        self
    }

    /// Ajoute une molecule entiere.
    pub fn entier(self, n: i64) -> Self {
        self.valeur(Valeur::Entier(n))
    }

    /// Ajoute plusieurs molecules entieres.
    pub fn entiers(mut self, valeurs: impl IntoIterator<Item = i64>) -> Self {
        for n in valeurs {
            self.ajouter_valeur(Valeur::Entier(n));
        }
        self
    }

    /// Ajoute une sous-solution.
    pub fn sous_solution(mut self, solution: Solution) -> Self {
        self.solution
            .elements
            .push(Element::Solution(Box::new(solution)));
        self
    }

    /// Ajout en place (pratique dans les boucles).
    pub fn ajouter_valeur(&mut self, valeur: Valeur) {
        let id = self.ids.molecule();
        self.solution
            .elements
            .push(Element::Molecule(Molecule { id, valeur }));
    }

    /// Ajout en place d'une sous-solution.
    pub fn ajouter_sous_solution(&mut self, solution: Solution) {
        self.solution
            .elements
            .push(Element::Solution(Box::new(solution)));
    }

    /// Ajout en place d'une regle.
    pub fn ajouter_regle(&mut self, regle: Arc<RegleCompilee>) {
        self.solution.regles.push(regle);
    }

    /// Termine la construction.
    pub fn construire(self) -> Solution {
        self.solution
    }
}
