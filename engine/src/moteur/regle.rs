//! Regles compilees et filtrage des reactifs.
//!
//! Une regle compilee est partagee (`Arc`) par toutes les solutions qui
//! l'activent : patron Poids-mouche (l'etat intrinseque, la regle, est
//! partage ; l'etat extrinseque, les molecules, reste dans chaque solution).

use crate::evaluateur::{ExprCompilee, Valeur};
use crate::solution::Element;
use crate::source::Span;
use std::sync::Arc;

/// Filtre de type d'un motif de valeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filtre {
    /// Toute valeur (jamais une sous-solution).
    Valeur,
    /// Entier seulement.
    Entier,
    /// Booleen seulement.
    Booleen,
    /// Chaine seulement.
    Chaine,
}

impl Filtre {
    /// Vrai si la valeur passe le filtre.
    #[inline]
    pub fn accepte(&self, valeur: &Valeur) -> bool {
        match self {
            Filtre::Valeur => true,
            Filtre::Entier => matches!(valeur, Valeur::Entier(_)),
            Filtre::Booleen => matches!(valeur, Valeur::Booleen(_)),
            Filtre::Chaine => matches!(valeur, Valeur::Chaine(_)),
        }
    }
}

/// Motif compile d'un reactif.
#[derive(Debug, Clone, PartialEq)]
pub enum MotifCompile {
    /// Lie une valeur a un emplacement.
    Valeur {
        /// Emplacement de l'environnement.
        emplacement: usize,
        /// Filtre de type.
        filtre: Filtre,
    },
    /// Destructure un n-uplet d'arite exacte.
    NUplet(Vec<MotifCompile>),
    /// Lie une sous-solution inerte.
    Solution,
}

impl MotifCompile {
    /// Pre-filtre rapide (type et arite seulement, sans liaison).
    #[inline]
    pub fn accepte(&self, element: &Element) -> bool {
        match (self, element) {
            (MotifCompile::Solution, Element::Solution(_)) => true,
            (MotifCompile::Solution, _) | (_, Element::Solution(_)) => false,
            (MotifCompile::Valeur { filtre, .. }, Element::Molecule(m)) => {
                filtre.accepte(&m.valeur)
            }
            (MotifCompile::NUplet(motifs), Element::Molecule(m)) => {
                matches!(&m.valeur, Valeur::NUplet(vs) if vs.len() == motifs.len())
            }
        }
    }

    /// Filtrage complet : verifie l'element et ecrit les liaisons.
    #[inline]
    pub fn filtrer(&self, element: &Element, emplacements: &mut [Valeur]) -> bool {
        match (self, element) {
            (MotifCompile::Solution, Element::Solution(_)) => true,
            (_, Element::Molecule(m)) => self.lier(&m.valeur, emplacements),
            _ => false,
        }
    }

    /// Lie une valeur (motifs de valeur et n-uplets) ; faux pour un motif
    /// de sous-solution.
    pub fn lier(&self, valeur: &Valeur, emplacements: &mut [Valeur]) -> bool {
        match self {
            MotifCompile::Valeur {
                emplacement,
                filtre,
            } => {
                if !filtre.accepte(valeur) {
                    return false;
                }
                emplacements[*emplacement] = valeur.clone();
                true
            }
            MotifCompile::NUplet(motifs) => match valeur {
                Valeur::NUplet(vs) if vs.len() == motifs.len() => motifs
                    .iter()
                    .zip(vs.iter())
                    .all(|(m, v)| m.lier(v, emplacements)),
                _ => false,
            },
            MotifCompile::Solution => false,
        }
    }
}

/// Produit compile.
#[derive(Debug, Clone, PartialEq)]
pub enum ProduitCompile {
    /// Nouvelle molecule dans la solution courante.
    Valeur(ExprCompilee),
    /// Dissolution du reactif sous-solution d'indice donne.
    Dissolution {
        /// Indice du reactif (position du motif).
        reactif: usize,
    },
    /// Injection d'une valeur dans le reactif sous-solution d'indice donne.
    Injection {
        /// Valeur injectee.
        valeur: ExprCompilee,
        /// Indice du reactif.
        reactif: usize,
    },
    /// Conservation telle quelle du reactif sous-solution (`by s`).
    Conservation {
        /// Indice du reactif.
        reactif: usize,
    },
    /// Injection asynchrone (`inject(e, s)` avec `s:ref`) : la valeur est
    /// deposee dans la boite aux lettres de la sous-solution referencee,
    /// qui la recoit pendant son execution (EF-D6).
    Envoi {
        /// Valeur envoyee.
        valeur: ExprCompilee,
        /// Rang de la reference (ordre des motifs `s:ref`).
        reference: usize,
    },
}

/// Regle prete a l'execution.
#[derive(Debug, Clone, PartialEq)]
pub struct RegleCompilee {
    /// Nom (unique dans le programme).
    pub nom: Arc<str>,
    /// Un motif par reactif consomme ; l'arite est `motifs.len()`.
    pub motifs: Vec<MotifCompile>,
    /// Condition facultative.
    pub condition: Option<ExprCompilee>,
    /// Produits.
    pub produits: Vec<ProduitCompile>,
    /// Nombre d'emplacements de l'environnement.
    pub emplacements: usize,
    /// Position de la definition dans le source.
    pub span: Span,
    /// Usage unique (`replace-one`) : chaque activation ne reagit qu'une
    /// fois, puis est retiree de la solution (EF-D5).
    pub unique: bool,
    /// Nombre de references `s:ref` (sous-solutions distinctes, non
    /// consommees, cibles des envois).
    pub references: usize,
}

impl RegleCompilee {
    /// Nombre de reactifs consommes (les references ne comptent pas).
    pub fn arite(&self) -> usize {
        self.motifs.len()
    }

    /// Vrai si la regle ne filtre aucun element (aucun reactif consomme) :
    /// elle est tentee une fois par passe, sans ancre.
    pub fn sans_reactif(&self) -> bool {
        self.motifs.is_empty()
    }
}
