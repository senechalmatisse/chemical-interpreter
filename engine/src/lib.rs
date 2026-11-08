//! Interpreteur de programmes chimiques (formalisme Gamma).
//!
//! Chaine de traitement :
//!
//! ```text
//! source .chem -> lexer -> parser (AST) -> semantique (passes + compilation)
//!   -> moteur de reaction (lots, reservation par propriete)
//!   <-> ordonnanceur (tours, sous-solutions et lots en parallele)
//!   -> evenements -> trace (JSONL, console) -> visualiseur web
//! ```
//!
//! La facade [`application::Interpreteur`] donne acces a toute la chaine ;
//! voir docs/architecture.md pour les principes SOLID et les patrons utilises.

pub mod alea;
pub mod annulation;
pub mod application;
pub mod ast;
pub mod cli;
pub mod diagnostic;
pub mod evaluateur;
pub mod lexer;
pub mod moteur;
pub mod ordonnanceur;
pub mod parser;
pub mod rejeu;
pub mod semantique;
pub mod solution;
pub mod source;
pub mod trace;

pub use application::{Interpreteur, ProgrammeCharge};
pub use ordonnanceur::{Configuration, Execution, Issue, Strategie};
