//! Trace d'execution : evenements, observateurs et formats de sortie.
//!
//! Le moteur publie des [`Evenement`]s via un [`Diffuseur`] ; les
//! observateurs les ecrivent en JSON Lines ([`TraceJsonl`]), les affichent
//! en console ([`TraceConsole`], [`PasAPas`]) ou les memorisent
//! ([`Enregistreur`]). Le [`ModeleTrace`] reconstruit l'etat des solutions a
//! partir des evenements (console, rejeu, visualiseur, tests).

mod console;
mod evenement;
mod format;
pub mod json;
mod jsonl;
mod modele;
mod observateur;

pub use console::{PasAPas, TraceConsole};
pub use evenement::{Evenement, VERSION_FORMAT, valeur_depuis_json, valeur_vers_json};
pub use format::{FormatInterne, FormatTrace};
pub use jsonl::TraceJsonl;
pub use modele::{FinTrace, ModeleTrace};
pub use observateur::{Diffuseur, Enregistreur, Observateur, TAILLE_LOT};
