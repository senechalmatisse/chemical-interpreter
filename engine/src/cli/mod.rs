//! Interface en ligne de commande.
//!
//! `principal` analyse les arguments, la fabrique ([`fabriquer`], patron
//! Fabrique) construit l'objet [`Commande`] correspondant, puis la commande
//! s'execute avec des entrees-sorties injectees. Codes de retour :
//! 0 succes, 1 erreur de syntaxe ou de semantique (ou d'utilisation),
//! 2 erreur d'execution, 3 garde-fou atteint.

pub mod arguments;
pub mod commandes;
pub mod serveur;

use crate::ordonnanceur::{Configuration, MAX_MOLECULES_DEFAUT, Strategie};
use arguments::{DefinitionCommande, DefinitionOption, aide, analyser};
use commandes::*;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

/// Codes de retour du processus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeRetour {
    /// Execution terminee (inertie) ou commande reussie.
    Succes = 0,
    /// Erreur de syntaxe, de semantique ou d'utilisation.
    ErreurProgramme = 1,
    /// Erreur d'execution (division par zero, E/S...).
    ErreurExecution = 2,
    /// Garde-fou atteint (ou interruption) avant l'inertie.
    GardeFou = 3,
}

/// Flux d'ecriture partageable entre la commande et les observateurs.
#[derive(Clone)]
pub struct SortiePartagee(Arc<Mutex<Box<dyn Write + Send>>>);

impl SortiePartagee {
    /// Enveloppe un flux.
    pub fn new(flux: Box<dyn Write + Send>) -> Self {
        Self(Arc::new(Mutex::new(flux)))
    }

    /// Flux en memoire et acces a son contenu (tests).
    pub fn memoire() -> (Self, Arc<Mutex<Vec<u8>>>) {
        let tampon = Arc::new(Mutex::new(Vec::new()));
        (Self::new(Box::new(Memoire(Arc::clone(&tampon)))), tampon)
    }
}

impl Write for SortiePartagee {
    fn write(&mut self, octets: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write(octets)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .flush()
    }
}

struct Memoire(Arc<Mutex<Vec<u8>>>);

impl Write for Memoire {
    fn write(&mut self, octets: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(octets);
        Ok(octets.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Entrees-sorties d'une commande.
pub struct EntreesSorties {
    /// Sortie standard.
    pub sortie: SortiePartagee,
    /// Sortie d'erreur.
    pub erreurs: SortiePartagee,
    /// Entree standard (mode pas-a-pas).
    pub entree: Option<Box<dyn BufRead + Send>>,
}

impl EntreesSorties {
    /// Flux standard du processus.
    pub fn standard() -> Self {
        Self {
            sortie: SortiePartagee::new(Box::new(io::stdout())),
            erreurs: SortiePartagee::new(Box::new(io::stderr())),
            entree: Some(Box::new(io::BufReader::new(io::stdin()))),
        }
    }
}

const fn opt(
    nom: &'static str,
    valeur: Option<&'static str>,
    aide: &'static str,
) -> DefinitionOption {
    DefinitionOption { nom, valeur, aide }
}

/// Declaration de `chem run`.
pub const RUN: DefinitionCommande = DefinitionCommande {
    nom: "run",
    positionnels: &["fichier.chem"],
    obligatoires: 1,
    options: &[
        opt(
            "seed",
            Some("N"),
            "graine du generateur (strategie aleatoire)",
        ),
        opt(
            "strategie",
            Some("aleatoire|ordonnee"),
            "ordre de selection des reactions (defaut : aleatoire)",
        ),
        opt(
            "workers",
            Some("N"),
            "nombre de workers (defaut : 1 = sequentiel)",
        ),
        opt("max-etapes", Some("N"), "arret apres N reactions"),
        opt(
            "delai-max",
            Some("DUREE"),
            "arret apres une duree (5, 1.5s, 500ms, 2m)",
        ),
        opt(
            "max-molecules",
            Some("N|aucune"),
            "taille maximale des solutions (defaut : 10000000)",
        ),
        opt("trace", None, "affiche la solution apres chaque etape"),
        opt(
            "trace-jsonl",
            Some("FICHIER"),
            "ecrit la trace JSON Lines ('-' : sortie standard)",
        ),
        opt(
            "pas-a-pas",
            None,
            "execution interactive, une reaction a la fois (force 1 worker)",
        ),
        opt(
            "stats",
            None,
            "affiche les statistiques sur la sortie d'erreur",
        ),
    ],
    resume: "Execute un programme chimique jusqu'a l'inertie et affiche la solution finale.",
};

/// Declaration de `chem check`.
pub const CHECK: DefinitionCommande = DefinitionCommande {
    nom: "check",
    positionnels: &["fichier.chem"],
    obligatoires: 1,
    options: &[],
    resume: "Verifie un programme (syntaxe, semantique, solution initiale) sans l'executer.",
};

/// Declaration de `chem fmt`.
pub const FMT: DefinitionCommande = DefinitionCommande {
    nom: "fmt",
    positionnels: &["fichier.chem"],
    obligatoires: 1,
    options: &[],
    resume: "Reimprime un programme sous forme normalisee (afficheur AST).",
};

/// Declaration de `chem replay`.
pub const REPLAY: DefinitionCommande = DefinitionCommande {
    nom: "replay",
    positionnels: &["trace.jsonl"],
    obligatoires: 1,
    options: &[opt(
        "programme",
        Some("FICHIER"),
        "re-execute et verifie chaque reaction avec ce programme",
    )],
    resume: "Rejoue une trace et affiche la solution finale reconstruite.",
};

/// Declaration de `chem viz`.
pub const VIZ: DefinitionCommande = DefinitionCommande {
    nom: "viz",
    positionnels: &["trace.jsonl"],
    obligatoires: 1,
    options: &[
        opt(
            "port",
            Some("N"),
            "port HTTP local (defaut : 8080, 0 = libre)",
        ),
        opt(
            "dist",
            Some("REPERTOIRE"),
            "visualiseur construit (defaut : visualizer/dist)",
        ),
        opt("sans-navigateur", None, "n'ouvre pas le navigateur"),
    ],
    resume: "Ouvre le visualiseur web sur une trace.",
};

const COMMANDES: [&DefinitionCommande; 5] = [&RUN, &CHECK, &FMT, &REPLAY, &VIZ];

/// Aide generale.
pub fn aide_generale() -> String {
    let mut texte = format!(
        "chem {} - interpreteur de programmes chimiques (formalisme Gamma)\n\nUsage : chem <commande> [options]\n\nCommandes :\n",
        env!("CARGO_PKG_VERSION")
    );
    for c in COMMANDES {
        texte.push_str(&format!("  {:<8} {}\n", c.nom, c.resume));
    }
    texte.push_str(
        "\nCodes de retour : 0 succes, 1 erreur de syntaxe ou de semantique, 2 erreur d'execution, 3 garde-fou.\n\
         Aide d'une commande : chem <commande> --help\n",
    );
    texte
}

fn configuration(a: &arguments::Arguments) -> Result<Configuration, String> {
    let strategie = match a.valeur("strategie") {
        None => Strategie::Aleatoire,
        Some(nom) => Strategie::depuis_nom(nom).ok_or_else(|| {
            format!("--strategie : 'aleatoire' ou 'ordonnee' attendu, trouve '{nom}'")
        })?,
    };
    let mut workers = a.entier("workers")?.unwrap_or(1).max(1) as usize;
    if a.drapeau("pas-a-pas") {
        workers = 1;
    }
    let max_molecules = match a.valeur("max-molecules") {
        Some("aucune") => None,
        Some(_) => a.entier("max-molecules")?,
        None => Some(MAX_MOLECULES_DEFAUT),
    };
    Ok(Configuration::constructeur()
        .strategie(strategie)
        .graine_optionnelle(a.entier("seed")?)
        .workers(workers)
        .max_etapes(a.entier("max-etapes")?)
        .delai_max(a.duree("delai-max")?)
        .max_molecules(max_molecules)
        .construire())
}

fn construire(
    definition: &DefinitionCommande,
    a: arguments::Arguments,
) -> Result<Box<dyn Commande>, String> {
    let premier = || PathBuf::from(&a.positionnels[0]);
    Ok(match definition.nom {
        "run" => Box::new(CommandeRun {
            fichier: premier(),
            configuration: configuration(&a)?,
            trace_console: a.drapeau("trace"),
            trace_jsonl: a.valeur("trace-jsonl").map(str::to_string),
            pas_a_pas: a.drapeau("pas-a-pas"),
            statistiques: a.drapeau("stats"),
        }),
        "check" => Box::new(CommandeCheck { fichier: premier() }),
        "fmt" => Box::new(CommandeFmt { fichier: premier() }),
        "replay" => Box::new(CommandeReplay {
            trace: premier(),
            programme: a.valeur("programme").map(PathBuf::from),
        }),
        "viz" => Box::new(CommandeViz {
            trace: premier(),
            port: match a.entier("port")? {
                None => 8080,
                Some(p) => {
                    u16::try_from(p).map_err(|_| format!("--port : {p} hors de 0..65535"))?
                }
            },
            dist: a
                .valeur("dist")
                .map_or_else(|| PathBuf::from("visualizer/dist"), PathBuf::from),
            ouvrir: !a.drapeau("sans-navigateur"),
        }),
        autre => return Err(format!("commande inconnue '{autre}'")),
    })
}

/// Fabrique : arguments (sans le nom du programme) -> commande.
pub fn fabriquer(arguments: &[String]) -> Box<dyn Commande> {
    let erreur = |message: String| -> Box<dyn Commande> { Box::new(CommandeErreur { message }) };
    let Some(nom) = arguments.first() else {
        return Box::new(CommandeTexte {
            texte: aide_generale(),
        });
    };
    match nom.as_str() {
        "-h" | "--help" | "help" => {
            return Box::new(CommandeTexte {
                texte: aide_generale(),
            });
        }
        "-V" | "--version" => {
            return Box::new(CommandeTexte {
                texte: format!("chem {}\n", env!("CARGO_PKG_VERSION")),
            });
        }
        _ => {}
    }
    let Some(definition) = COMMANDES.iter().find(|c| c.nom == nom) else {
        let mut message = format!("commande inconnue '{nom}'");
        if let Some(proche) = crate::semantique::suggestion(nom, COMMANDES.iter().map(|c| c.nom)) {
            message.push_str(&format!(" (vouliez-vous dire '{proche}' ?)"));
        }
        return erreur(message);
    };
    match analyser(definition, &arguments[1..]) {
        Ok(a) if a.aide => Box::new(CommandeTexte {
            texte: aide(definition),
        }),
        Ok(a) => construire(definition, a).unwrap_or_else(erreur),
        Err(message) => erreur(message),
    }
}

/// Point d'entree testable : retourne le code de sortie.
pub fn principal(arguments: &[String], es: &mut EntreesSorties) -> i32 {
    let code = fabriquer(arguments).executer(es);
    let _ = es.sortie.flush();
    let _ = es.erreurs.flush();
    code as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lancer(arguments: &[&str]) -> (i32, String, String) {
        let (sortie, tampon_sortie) = SortiePartagee::memoire();
        let (erreurs, tampon_erreurs) = SortiePartagee::memoire();
        let mut es = EntreesSorties {
            sortie,
            erreurs,
            entree: None,
        };
        let args: Vec<String> = arguments.iter().map(|s| s.to_string()).collect();
        let code = principal(&args, &mut es);
        let texte = |t: &Arc<Mutex<Vec<u8>>>| String::from_utf8(t.lock().unwrap().clone()).unwrap();
        (code, texte(&tampon_sortie), texte(&tampon_erreurs))
    }

    #[test]
    fn aide_version_et_erreurs_d_utilisation() {
        let (code, sortie, _) = lancer(&[]);
        assert_eq!(code, 0);
        assert!(sortie.contains("Commandes"));
        assert_eq!(lancer(&["--version"]).0, 0);
        let (code, sortie, _) = lancer(&["run", "--help"]);
        assert_eq!(code, 0);
        assert!(sortie.contains("--max-etapes <N>"));
        let (code, _, erreurs) = lancer(&["rnu"]);
        assert_eq!(code, 1);
        assert!(erreurs.contains("'run'"));
        assert_eq!(lancer(&["run"]).0, 1);
        assert_eq!(lancer(&["run", "x.chem", "--strategie", "hasard"]).0, 1);
        let (code, _, erreurs) = lancer(&["run", "/inexistant.chem"]);
        assert_eq!(code, 1);
        assert!(erreurs.contains("impossible de lire"));
    }
}
