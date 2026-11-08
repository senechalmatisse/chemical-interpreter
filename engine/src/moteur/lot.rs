//! Lot : portion d'une solution confiee a un worker.
//!
//! Un lot possede ses elements (propriete exclusive au sens de Rust) : la
//! reservation des reactifs est donc garantie statiquement, sans verrou,
//! tant que le lot est traite par un seul worker (reservation par lots).

use crate::solution::{Element, Solution};

/// Elements d'une solution en cours de reaction.
#[derive(Debug)]
pub struct Lot {
    /// Elements ; `None` marque un element consomme (compacte entre passes).
    pub(crate) elements: Vec<Option<Element>>,
    /// Sous-solutions reactivees par injection : plus liables jusqu'a leur
    /// prochaine inertie. Les boites sont conservees telles quelles entre
    /// `Element::Solution` et ce vecteur (aucune reallocation).
    #[allow(clippy::vec_box)]
    pub(crate) reactivees: Vec<Box<Solution>>,
    /// Graine propre au lot (strategie aleatoire).
    pub(crate) graine: u64,
}

impl Lot {
    /// Lot initial.
    pub fn new(elements: Vec<Element>, graine: u64) -> Self {
        Self {
            elements: elements.into_iter().map(Some).collect(),
            reactivees: Vec::new(),
            graine,
        }
    }

    /// Supprime les emplacements consommes (l'ordre relatif est conserve).
    pub fn compacter(&mut self) {
        self.elements.retain(Option::is_some);
    }

    /// Nombre d'elements vivants.
    pub fn taille(&self) -> usize {
        self.elements.iter().filter(|e| e.is_some()).count()
    }

    /// Absorbe un autre lot (fusion de partitions).
    pub fn absorber(&mut self, autre: Lot) {
        self.elements
            .extend(autre.elements.into_iter().filter(Option::is_some));
        self.reactivees.extend(autre.reactivees);
    }

    /// Restitue les elements (les sous-solutions reactivees en dernier).
    pub fn en_elements(self) -> Vec<Element> {
        self.elements
            .into_iter()
            .flatten()
            .chain(self.reactivees.into_iter().map(Element::Solution))
            .collect()
    }
}
