//! Facade de l'interpreteur.
//!
//! [`Interpreteur`] masque la chaine complete (lexer, parser, analyse
//! semantique, compilation, ordonnancement) derriere quelques operations de
//! haut niveau. La ligne de commande, les tests et les benchmarks n'utilisent
//! que cette facade.

use crate::annulation::JetonAnnulation;
use crate::ast::Programme;
use crate::diagnostic::Diagnostics;
use crate::ordonnanceur::{
    Configuration, Execution, FabriqueParallelisme, Ordonnanceur, ParallelismeStandard,
};
use crate::parser;
use crate::semantique::{self, Analyseur, ProgrammeCompile};
use crate::source::Source;
use crate::trace::Diffuseur;
use std::sync::Arc;

/// Programme analyse et compile, pret a l'execution.
#[derive(Debug)]
pub struct ProgrammeCharge {
    /// Nom de la source.
    pub nom: String,
    /// AST.
    pub ast: Programme,
    /// Forme executable.
    pub compile: ProgrammeCompile,
    /// Avertissements de l'analyse.
    pub avertissements: Diagnostics,
}

/// Facade de l'interpreteur.
pub struct Interpreteur {
    analyseur: Analyseur,
    parallelisme: Arc<dyn FabriqueParallelisme>,
}

impl Default for Interpreteur {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreteur {
    /// Interpreteur avec les verifications standard.
    pub fn new() -> Self {
        Self::avec_analyseur(Analyseur::standard())
    }

    /// Interpreteur avec une chaine de verifications personnalisee.
    pub fn avec_analyseur(analyseur: Analyseur) -> Self {
        Self {
            analyseur,
            parallelisme: Arc::new(ParallelismeStandard),
        }
    }

    /// Implementation du pool de workers utilisee par les executions.
    pub fn avec_parallelisme(mut self, fabrique: Arc<dyn FabriqueParallelisme>) -> Self {
        self.parallelisme = fabrique;
        self
    }

    /// Analyse syntaxique seule.
    pub fn analyser_syntaxe(&self, source: &Source) -> Result<Programme, Diagnostics> {
        parser::analyser_programme(source).map_err(Diagnostics::from)
    }

    /// Analyse syntaxique et semantique ; retourne l'AST et les avertissements.
    pub fn verifier(&self, source: &Source) -> Result<(Programme, Diagnostics), Diagnostics> {
        let programme = self.analyser_syntaxe(source)?;
        let diagnostics = self.analyseur.analyser(&programme);
        if diagnostics.a_des_erreurs() {
            Err(diagnostics)
        } else {
            Ok((programme, diagnostics))
        }
    }

    /// Analyse et compile ; `max_molecules` borne la solution initiale.
    pub fn charger(
        &self,
        source: &Source,
        max_molecules: Option<u64>,
    ) -> Result<ProgrammeCharge, Diagnostics> {
        let (ast, avertissements) = self.verifier(source)?;
        let compile = semantique::compiler(&ast, max_molecules.unwrap_or(u64::MAX))?;
        Ok(ProgrammeCharge {
            nom: source.nom().to_string(),
            ast,
            compile,
            avertissements,
        })
    }

    /// Execute un programme charge.
    pub fn executer(
        &self,
        programme: ProgrammeCharge,
        configuration: &Configuration,
        diffuseur: &Diffuseur,
        annulation: Option<JetonAnnulation>,
    ) -> Execution {
        let mut ordonnanceur = Ordonnanceur::new(configuration, diffuseur)
            .avec_programme(programme.nom)
            .avec_parallelisme(&*self.parallelisme);
        if let Some(jeton) = annulation {
            ordonnanceur = ordonnanceur.avec_annulation(jeton);
        }
        let ProgrammeCompile { solution, ids, .. } = programme.compile;
        ordonnanceur.executer(solution, &ids)
    }

    /// Raccourci : texte source -> execution sans trace.
    pub fn executer_texte(
        &self,
        texte: &str,
        configuration: &Configuration,
    ) -> Result<Execution, Diagnostics> {
        let source = Source::new("<texte>", texte);
        let programme = self.charger(&source, configuration.limites().max_molecules)?;
        Ok(self.executer(programme, configuration, &Diffuseur::new(), None))
    }
}
