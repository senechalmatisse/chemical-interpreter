//! Points d'integration : la ligne de commande utilise les implementations
//! reunies dans `Composants` (analyse des arguments, rendu des diagnostics,
//! format de trace, pool de workers).

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chem::cli::arguments::{AnalyseurArguments, AnalyseurInterne, Arguments, DefinitionCommande};
use chem::cli::{Composants, EntreesSorties, SortiePartagee, principal_avec};
use chem::diagnostic::{Diagnostic, RenduDiagnostic, RenduTexte};
use chem::ordonnanceur::{FabriqueParallelisme, Parallelisme, Pool};
use chem::source::Source;
use chem::trace::{Evenement, FormatInterne, FormatTrace};

/// Compte les appels de chaque point d'integration.
#[derive(Default)]
struct Compteurs {
    analyses: AtomicUsize,
    rendus: AtomicUsize,
    ecritures: AtomicUsize,
    lectures: AtomicUsize,
    pools: AtomicUsize,
}

struct Analyseur(Arc<Compteurs>);

impl AnalyseurArguments for Analyseur {
    fn nom(&self) -> &'static str {
        "analyseur de test"
    }

    fn analyser(
        &self,
        definition: &DefinitionCommande,
        arguments: &[String],
    ) -> Result<Arguments, String> {
        self.0.analyses.fetch_add(1, Ordering::SeqCst);
        AnalyseurInterne.analyser(definition, arguments)
    }
}

struct Rendu(Arc<Compteurs>);

impl RenduDiagnostic for Rendu {
    fn nom(&self) -> &'static str {
        "rendu de test"
    }

    fn rendre(&self, diagnostic: &Diagnostic, source: &Source) -> String {
        self.0.rendus.fetch_add(1, Ordering::SeqCst);
        format!("[test] {}", RenduTexte.rendre(diagnostic, source))
    }
}

struct Format(Arc<Compteurs>);

impl FormatTrace for Format {
    fn nom(&self) -> &'static str {
        "format de test"
    }

    fn ecrire(&self, evenement: &Evenement, sortie: &mut Vec<u8>) -> std::io::Result<()> {
        self.0.ecritures.fetch_add(1, Ordering::SeqCst);
        FormatInterne.ecrire(evenement, sortie)
    }

    fn lire(&self, ligne: &str) -> Result<Evenement, String> {
        self.0.lectures.fetch_add(1, Ordering::SeqCst);
        FormatInterne.lire(ligne)
    }
}

struct Fabrique(Arc<Compteurs>);

impl FabriqueParallelisme for Fabrique {
    fn nom(&self) -> &'static str {
        "pool de test"
    }

    fn creer(&self, workers: usize) -> Box<dyn Parallelisme> {
        self.0.pools.fetch_add(1, Ordering::SeqCst);
        Box::new(Pool::new(workers))
    }
}

fn composants() -> (Composants, Arc<Compteurs>) {
    let compteurs = Arc::new(Compteurs::default());
    let composants = Composants {
        arguments: Arc::new(Analyseur(compteurs.clone())),
        diagnostics: Arc::new(Rendu(compteurs.clone())),
        trace: Arc::new(Format(compteurs.clone())),
        parallelisme: Arc::new(Fabrique(compteurs.clone())),
    };
    (composants, compteurs)
}

fn lancer(arguments: &[&str], composants: &Composants) -> (i32, String, String) {
    let (sortie, tampon_sortie) = SortiePartagee::memoire();
    let (erreurs, tampon_erreurs) = SortiePartagee::memoire();
    let mut es = EntreesSorties {
        sortie,
        erreurs,
        entree: None,
    };
    let arguments: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
    let code = principal_avec(&arguments, &mut es, composants);
    let texte = |t: &Arc<Mutex<Vec<u8>>>| String::from_utf8(t.lock().unwrap().clone()).unwrap();
    (code, texte(&tampon_sortie), texte(&tampon_erreurs))
}

fn temporaire(nom: &str, contenu: &str) -> PathBuf {
    let repertoire = std::env::temp_dir().join(format!("chem-composants-{}", std::process::id()));
    std::fs::create_dir_all(&repertoire).unwrap();
    let chemin = repertoire.join(nom);
    std::fs::write(&chemin, contenu).unwrap();
    chemin
}

#[test]
fn version_liste_les_implementations() {
    let (composants, _) = composants();
    let (code, sortie, _) = lancer(&["--version"], &composants);
    assert_eq!(code, 0);
    for nom in [
        "analyseur de test",
        "rendu de test",
        "format de test",
        "pool de test",
    ] {
        assert!(sortie.contains(nom), "{nom} absent de :\n{sortie}");
    }
}

#[test]
fn diagnostics_rendus_par_le_composant() {
    let (composants, compteurs) = composants();
    let programme = temporaire(
        "invalide.chem",
        "let max = replace x, y by x ;\n{ 1, 2 | max, inconnue }\n",
    );
    let (code, _, erreurs) = lancer(&["check", programme.to_str().unwrap()], &composants);
    assert_eq!(code, 1);
    assert!(erreurs.starts_with("[test] "), "{erreurs}");
    assert!(compteurs.rendus.load(Ordering::SeqCst) >= 1);
    assert_eq!(compteurs.analyses.load(Ordering::SeqCst), 1);
}

#[test]
fn execution_trace_et_rejeu_par_les_composants() {
    let (composants, compteurs) = composants();
    let programme = temporaire(
        "maximum.chem",
        "let max = replace x, y by x if x >= y ;\n{ 3, 9, 4, 1, 7 | max }\n",
    );
    let trace = programme.with_file_name("maximum.jsonl");
    let (code, sortie, erreurs) = lancer(
        &[
            "run",
            programme.to_str().unwrap(),
            "--workers",
            "2",
            "--seed",
            "5",
            "--trace-jsonl",
            trace.to_str().unwrap(),
        ],
        &composants,
    );
    assert_eq!(code, 0, "{erreurs}");
    assert_eq!(sortie, "{ 9 | max }\n");
    assert_eq!(compteurs.pools.load(Ordering::SeqCst), 1);
    let ecritures = compteurs.ecritures.load(Ordering::SeqCst);
    assert!(ecritures > 0);

    let (code, rejeu, erreurs) = lancer(
        &[
            "replay",
            trace.to_str().unwrap(),
            "--programme",
            programme.to_str().unwrap(),
        ],
        &composants,
    );
    assert_eq!(code, 0, "{erreurs}");
    assert!(rejeu.contains("{ 9 | max }"), "{rejeu}");
    assert_eq!(compteurs.lectures.load(Ordering::SeqCst), ecritures);
    assert_eq!(compteurs.analyses.load(Ordering::SeqCst), 2);
}
