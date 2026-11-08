//! Commandes de la CLI : chaque sous-commande est un objet
//! autonome, construit par la fabrique de [`super::fabriquer`] et execute
//! avec des entrees-sorties injectees (testables sans terminal).

use super::{CodeRetour, Composants, EntreesSorties};
use crate::annulation::JetonAnnulation;
use crate::application::Interpreteur;
use crate::diagnostic::{Diagnostics, RenduDiagnostic};
use crate::moteur::Arret;
use crate::ordonnanceur::{Configuration, Issue};
use crate::rejeu;
use crate::source::Source;
use crate::trace::{Diffuseur, PasAPas, TraceConsole, TraceJsonl};
use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};

/// Commande executable.
pub trait Commande {
    /// Execute la commande et retourne le code de sortie.
    fn executer(self: Box<Self>, es: &mut EntreesSorties) -> CodeRetour;
}

fn lire_source(chemin: &Path, es: &mut EntreesSorties) -> Result<Source, CodeRetour> {
    Source::depuis_fichier(chemin).map_err(|e| {
        let _ = writeln!(
            es.erreurs,
            "erreur : impossible de lire '{}' : {e}",
            chemin.display()
        );
        CodeRetour::ErreurProgramme
    })
}

fn afficher_diagnostics(
    diagnostics: &Diagnostics,
    source: &Source,
    es: &mut EntreesSorties,
    rendu: &dyn RenduDiagnostic,
) {
    for d in diagnostics.tries() {
        let _ = write!(es.erreurs, "{}", rendu.rendre(d, source));
    }
    let erreurs = diagnostics.erreurs().count();
    if erreurs > 1 {
        let _ = writeln!(es.erreurs, "{erreurs} erreurs");
    }
}

/// `chem run` : execute un programme.
pub struct CommandeRun {
    /// Programme.
    pub fichier: PathBuf,
    /// Parametres d'execution.
    pub configuration: Configuration,
    /// Affichage de la solution a chaque etape.
    pub trace_console: bool,
    /// Fichier de trace JSON Lines (`-` : sortie standard).
    pub trace_jsonl: Option<String>,
    /// Mode pas-a-pas interactif.
    pub pas_a_pas: bool,
    /// Affichage des statistiques.
    pub statistiques: bool,
    /// Implementations des points d'integration.
    pub composants: Composants,
}

impl Commande for CommandeRun {
    fn executer(self: Box<Self>, es: &mut EntreesSorties) -> CodeRetour {
        let source = match lire_source(&self.fichier, es) {
            Ok(s) => s,
            Err(code) => return code,
        };
        let rendu = &*self.composants.diagnostics;
        let interpreteur =
            Interpreteur::new().avec_parallelisme(self.composants.parallelisme.clone());
        let programme =
            match interpreteur.charger(&source, self.configuration.limites().max_molecules) {
                Ok(p) => p,
                Err(diagnostics) => {
                    afficher_diagnostics(&diagnostics, &source, es, rendu);
                    return CodeRetour::ErreurProgramme;
                }
            };
        afficher_diagnostics(&programme.avertissements, &source, es, rendu);

        let annulation = JetonAnnulation::new();
        let mut diffuseur = Diffuseur::new();
        if self.pas_a_pas {
            let entree = es.entree.take().unwrap_or_else(|| Box::new(io::empty()));
            diffuseur.abonner(Box::new(PasAPas::new(
                TraceConsole::new(Box::new(es.sortie.clone())),
                entree,
                Box::new(es.sortie.clone()),
                annulation.clone(),
            )));
        } else if self.trace_console {
            diffuseur.abonner(Box::new(TraceConsole::new(Box::new(es.sortie.clone()))));
        }
        let trace_sur_sortie = self.trace_jsonl.as_deref() == Some("-");
        if let Some(chemin) = &self.trace_jsonl {
            let trace = if trace_sur_sortie {
                TraceJsonl::new(Box::new(es.sortie.clone()))
            } else {
                match TraceJsonl::fichier(Path::new(chemin)) {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = writeln!(
                            es.erreurs,
                            "erreur : impossible de creer la trace '{chemin}' : {e}"
                        );
                        return CodeRetour::ErreurExecution;
                    }
                }
            };
            diffuseur.abonner(Box::new(trace.avec_format(self.composants.trace.clone())));
        }

        let execution =
            interpreteur.executer(programme, &self.configuration, &diffuseur, Some(annulation));
        if let Err(e) = diffuseur.terminer() {
            let _ = writeln!(es.erreurs, "erreur : ecriture de la trace : {e}");
            return CodeRetour::ErreurExecution;
        }
        if trace_sur_sortie {
            let _ = writeln!(es.erreurs, "{}", execution.solution);
        } else {
            let _ = writeln!(es.sortie, "{}", execution.solution);
        }
        if self.statistiques {
            let _ = writeln!(es.erreurs, "{}", execution.statistiques);
        }
        match &execution.issue {
            Issue::Inerte => CodeRetour::Succes,
            Issue::Arretee(Arret::Erreur(erreur)) => {
                let _ = write!(
                    es.erreurs,
                    "{}",
                    rendu.rendre(&erreur.diagnostic(), &source)
                );
                CodeRetour::ErreurExecution
            }
            Issue::Arretee(arret) => {
                let _ = writeln!(es.erreurs, "garde-fou : {arret}");
                let _ = writeln!(
                    es.erreurs,
                    "  la solution affichee est l'etat au moment de l'arret (non inerte)"
                );
                CodeRetour::GardeFou
            }
        }
    }
}

/// `chem check` : verifie un programme sans l'executer.
pub struct CommandeCheck {
    /// Programme.
    pub fichier: PathBuf,
    /// Implementations des points d'integration.
    pub composants: Composants,
}

impl Commande for CommandeCheck {
    fn executer(self: Box<Self>, es: &mut EntreesSorties) -> CodeRetour {
        let source = match lire_source(&self.fichier, es) {
            Ok(s) => s,
            Err(code) => return code,
        };
        let rendu = &*self.composants.diagnostics;
        match Interpreteur::new().charger(&source, None) {
            Ok(p) => {
                afficher_diagnostics(&p.avertissements, &source, es, rendu);
                let _ = writeln!(
                    es.sortie,
                    "ok : {} regle(s), {} molecule(s) et {} solution(s) initiales",
                    p.compile.regles.len(),
                    p.compile.solution.nombre_molecules(),
                    p.compile.solution.nombre_solutions()
                );
                CodeRetour::Succes
            }
            Err(diagnostics) => {
                afficher_diagnostics(&diagnostics, &source, es, rendu);
                CodeRetour::ErreurProgramme
            }
        }
    }
}

/// `chem fmt` : reimprime un programme sous forme normalisee.
pub struct CommandeFmt {
    /// Programme.
    pub fichier: PathBuf,
    /// Implementations des points d'integration.
    pub composants: Composants,
}

impl Commande for CommandeFmt {
    fn executer(self: Box<Self>, es: &mut EntreesSorties) -> CodeRetour {
        let source = match lire_source(&self.fichier, es) {
            Ok(s) => s,
            Err(code) => return code,
        };
        match Interpreteur::new().analyser_syntaxe(&source) {
            Ok(programme) => {
                let _ = write!(es.sortie, "{programme}");
                CodeRetour::Succes
            }
            Err(diagnostics) => {
                afficher_diagnostics(&diagnostics, &source, es, &*self.composants.diagnostics);
                CodeRetour::ErreurProgramme
            }
        }
    }
}

/// `chem replay` : rejoue (et verifie) une trace.
pub struct CommandeReplay {
    /// Trace JSON Lines.
    pub trace: PathBuf,
    /// Programme pour la verification des reactions.
    pub programme: Option<PathBuf>,
    /// Implementations des points d'integration.
    pub composants: Composants,
}

impl Commande for CommandeReplay {
    fn executer(self: Box<Self>, es: &mut EntreesSorties) -> CodeRetour {
        let compile = match &self.programme {
            None => None,
            Some(chemin) => {
                let source = match lire_source(chemin, es) {
                    Ok(s) => s,
                    Err(code) => return code,
                };
                match Interpreteur::new().charger(&source, None) {
                    Ok(p) => Some(p.compile),
                    Err(diagnostics) => {
                        afficher_diagnostics(
                            &diagnostics,
                            &source,
                            es,
                            &*self.composants.diagnostics,
                        );
                        return CodeRetour::ErreurProgramme;
                    }
                }
            }
        };
        let fichier = match File::open(&self.trace) {
            Ok(f) => f,
            Err(e) => {
                let _ = writeln!(
                    es.erreurs,
                    "erreur : impossible de lire '{}' : {e}",
                    self.trace.display()
                );
                return CodeRetour::ErreurProgramme;
            }
        };
        match rejeu::rejouer_avec_format(
            BufReader::new(fichier),
            compile.as_ref().map(|c| &c.regles),
            &*self.composants.trace,
        ) {
            Ok(r) => {
                let _ = writeln!(
                    es.sortie,
                    "{}",
                    r.modele.racine_canonique().unwrap_or_default()
                );
                let verification = if compile.is_some() {
                    format!(
                        ", {} reaction(s) re-executee(s) et verifiee(s)",
                        r.reactions_verifiees
                    )
                } else {
                    String::new()
                };
                let _ = writeln!(
                    es.erreurs,
                    "rejeu : {} evenement(s), {} reaction(s){verification}",
                    r.evenements, r.modele.reactions
                );
                match &r.modele.fin {
                    Some(fin) if fin.inerte => CodeRetour::Succes,
                    Some(fin) => {
                        let _ = writeln!(
                            es.erreurs,
                            "la trace se termine par un arret ({})",
                            fin.motif
                        );
                        CodeRetour::GardeFou
                    }
                    None => {
                        let _ = writeln!(
                            es.erreurs,
                            "avertissement : trace incomplete (evenement 'fin' absent)"
                        );
                        CodeRetour::Succes
                    }
                }
            }
            Err(erreur) => {
                let _ = writeln!(
                    es.erreurs,
                    "erreur de rejeu, {}: {erreur}",
                    self.trace.display()
                );
                if erreur.format {
                    CodeRetour::ErreurProgramme
                } else {
                    CodeRetour::ErreurExecution
                }
            }
        }
    }
}

/// `chem viz` : sert le visualiseur web avec une trace.
pub struct CommandeViz {
    /// Trace JSON Lines.
    pub trace: PathBuf,
    /// Port HTTP local.
    pub port: u16,
    /// Repertoire du visualiseur construit (`visualizer/dist`).
    pub dist: PathBuf,
    /// Ouvre le navigateur.
    pub ouvrir: bool,
}

impl Commande for CommandeViz {
    fn executer(self: Box<Self>, es: &mut EntreesSorties) -> CodeRetour {
        if !self.trace.is_file() {
            let _ = writeln!(
                es.erreurs,
                "erreur : trace '{}' introuvable",
                self.trace.display()
            );
            return CodeRetour::ErreurProgramme;
        }
        if !self.dist.join("index.html").is_file() {
            let _ = writeln!(
                es.erreurs,
                "erreur : visualiseur introuvable dans '{}'\n  construisez-le : cd visualizer && npm install && npm run build\n  ou indiquez son emplacement : --dist <repertoire>",
                self.dist.display()
            );
            return CodeRetour::ErreurProgramme;
        }
        match super::serveur::servir(
            &self.dist,
            &self.trace,
            self.port,
            self.ouvrir,
            &mut es.sortie,
        ) {
            Ok(()) => CodeRetour::Succes,
            Err(e) => {
                let _ = writeln!(es.erreurs, "erreur du serveur : {e}");
                CodeRetour::ErreurExecution
            }
        }
    }
}

/// Affiche un texte fixe (aide, version).
pub struct CommandeTexte {
    /// Texte a afficher.
    pub texte: String,
}

impl Commande for CommandeTexte {
    fn executer(self: Box<Self>, es: &mut EntreesSorties) -> CodeRetour {
        let _ = write!(es.sortie, "{}", self.texte);
        CodeRetour::Succes
    }
}

/// Erreur d'utilisation de la ligne de commande.
pub struct CommandeErreur {
    /// Message.
    pub message: String,
}

impl Commande for CommandeErreur {
    fn executer(self: Box<Self>, es: &mut EntreesSorties) -> CodeRetour {
        let _ = writeln!(
            es.erreurs,
            "erreur : {}\n  voir 'chem --help'",
            self.message
        );
        CodeRetour::ErreurProgramme
    }
}
