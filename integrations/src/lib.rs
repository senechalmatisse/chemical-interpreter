//! Implementations des points d'integration du moteur avec des
//! bibliotheques de l'ecosysteme Rust.
//!
//! Le moteur (paquet `chem`) n'a aucune dependance : chaque service qui
//! pourrait venir d'une bibliotheque externe y est decrit par une interface
//! et dispose d'une implementation avec la bibliotheque standard. Ce paquet
//! fournit une autre implementation par interface, chacune dans son module
//! et derriere sa fonctionnalite Cargo :
//!
//! | Fonctionnalite | Interface du moteur                         | Implementation               |
//! |----------------|---------------------------------------------|------------------------------|
//! | `rayon`        | `chem::ordonnanceur::FabriqueParallelisme`  | `parallelisme::FabriqueRayon` |
//! | `clap`         | `chem::cli::arguments::AnalyseurArguments`  | `arguments::AnalyseurClap`   |
//! | `serde`        | `chem::trace::FormatTrace`                  | `trace::FormatSerde`         |
//! | `miette`       | `chem::diagnostic::RenduDiagnostic`         | `diagnostics::RenduMiette`   |
//!
//! Aucun module ne depend d'un autre : retirer une fonctionnalite retire sa
//! dependance et rend au point d'integration son implementation standard.
//! [`composants`] reunit les implementations des fonctionnalites actives ;
//! l'executable du paquet est la commande `chem` construite avec elles.
//!
//! Utilisation depuis un autre programme :
//!
//! ```no_run
//! let arguments: Vec<String> = std::env::args().skip(1).collect();
//! let mut es = chem::cli::EntreesSorties::standard();
//! let code = chem::cli::principal_avec(&arguments, &mut es, &chem_integrations::composants());
//! std::process::exit(code);
//! ```

#[cfg(feature = "clap")]
pub mod arguments;
#[cfg(feature = "miette")]
pub mod diagnostics;
#[cfg(feature = "rayon")]
pub mod parallelisme;
#[cfg(feature = "serde")]
pub mod trace;

use chem::cli::Composants;
use chem::cli::arguments::AnalyseurArguments;
use chem::diagnostic::RenduDiagnostic;
use chem::ordonnanceur::FabriqueParallelisme;
use chem::trace::FormatTrace;
use std::sync::Arc;

/// Composants de la ligne de commande : l'implementation de chaque
/// fonctionnalite active, celle de la bibliotheque pour les autres.
pub fn composants() -> Composants {
    Composants {
        arguments: analyseur_arguments(),
        diagnostics: rendu_diagnostics(),
        trace: format_trace(),
        parallelisme: fabrique_parallelisme(),
    }
}

#[cfg(feature = "clap")]
fn analyseur_arguments() -> Arc<dyn AnalyseurArguments> {
    Arc::new(arguments::AnalyseurClap)
}

#[cfg(not(feature = "clap"))]
fn analyseur_arguments() -> Arc<dyn AnalyseurArguments> {
    Composants::standard().arguments
}

#[cfg(feature = "miette")]
fn rendu_diagnostics() -> Arc<dyn RenduDiagnostic> {
    Arc::new(diagnostics::RenduMiette::automatique())
}

#[cfg(not(feature = "miette"))]
fn rendu_diagnostics() -> Arc<dyn RenduDiagnostic> {
    Composants::standard().diagnostics
}

#[cfg(feature = "serde")]
fn format_trace() -> Arc<dyn FormatTrace> {
    Arc::new(trace::FormatSerde)
}

#[cfg(not(feature = "serde"))]
fn format_trace() -> Arc<dyn FormatTrace> {
    Composants::standard().trace
}

#[cfg(feature = "rayon")]
fn fabrique_parallelisme() -> Arc<dyn FabriqueParallelisme> {
    Arc::new(parallelisme::FabriqueRayon)
}

#[cfg(not(feature = "rayon"))]
fn fabrique_parallelisme() -> Arc<dyn FabriqueParallelisme> {
    Composants::standard().parallelisme
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_implementation_par_fonctionnalite_active() {
        let composants = composants();
        let standard = Composants::standard();
        let attendu = |active: bool, nom: &'static str, defaut: &'static str| {
            if active { nom } else { defaut }
        };
        assert_eq!(
            composants.arguments.nom(),
            attendu(cfg!(feature = "clap"), "clap", standard.arguments.nom())
        );
        assert_eq!(
            composants.diagnostics.nom(),
            attendu(
                cfg!(feature = "miette"),
                "miette",
                standard.diagnostics.nom()
            )
        );
        assert_eq!(
            composants.trace.nom(),
            attendu(cfg!(feature = "serde"), "serde_json", standard.trace.nom())
        );
        assert_eq!(
            composants.parallelisme.nom(),
            attendu(
                cfg!(feature = "rayon"),
                "rayon",
                standard.parallelisme.nom()
            )
        );
    }
}
