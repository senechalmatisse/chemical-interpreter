//! Moteur de reaction : regles compilees, filtrage, reaction d'un lot.
//!
//! Le moteur decrit *comment* une portion de solution reagit (sequentiel,
//! un worker). L'[`crate::ordonnanceur`] decide *quand* et *ou* : tours,
//! sous-solutions en parallele, partitions.

mod combinaisons;
mod contexte;
pub mod jointure;
mod lot;
mod reacteur;
mod regle;

pub use combinaisons::{Combinaisons, Permutations};
pub use contexte::{Arret, Compteurs, ContexteExecution, ErreurExecution, GardeFous};
pub use jointure::Jointure;
pub use lot::Lot;
pub use reacteur::Reacteur;
pub use regle::{Filtre, MotifCompile, ProduitCompile, RegleCompilee};
