//! Mesures de performance : `cargo bench` ou `cargo bench -- <filtre>`.
//!
//! Harnais maison (Criterion n'est pas disponible hors ligne) : une
//! execution d'echauffement puis `REPETITIONS` mesures ; on rapporte la
//! mediane et le minimum du temps d'execution du moteur (hors analyse), puis
//! l'acceleration `S = T1 / Tn` et l'efficacite `E = S / n`.
//! Le surcout de la trace JSON Lines (EF-V7) est mesure a part.

use chem::trace::{Diffuseur, TraceJsonl};
use chem::{Configuration, Interpreteur, Strategie};
use std::io;
use std::path::Path;
use std::time::Duration;

const REPETITIONS: usize = 5;

fn charger(chemin: &Path) -> String {
    std::fs::read_to_string(chemin).unwrap_or_else(|e| panic!("{} : {e}", chemin.display()))
}

fn mesurer(texte: &str, workers: usize, trace: bool) -> Duration {
    let interpreteur = Interpreteur::new();
    let config = Configuration::constructeur()
        .strategie(Strategie::Aleatoire)
        .graine(2026)
        .workers(workers)
        .max_molecules(None)
        .construire();
    let source = chem::source::Source::new("bench", texte);
    let programme = interpreteur
        .charger(&source, None)
        .expect("programme valide");
    let diffuseur = if trace {
        Diffuseur::new().avec(TraceJsonl::new(Box::new(io::sink())))
    } else {
        Diffuseur::new()
    };
    let execution = interpreteur.executer(programme, &config, &diffuseur, None);
    assert!(execution.est_inerte());
    std::hint::black_box(&execution.solution);
    execution.statistiques.duree
}

fn serie(texte: &str, workers: usize, trace: bool) -> (f64, f64) {
    mesurer(texte, workers, trace);
    let mut temps: Vec<f64> = (0..REPETITIONS)
        .map(|_| mesurer(texte, workers, trace).as_secs_f64() * 1000.0)
        .collect();
    temps.sort_by(f64::total_cmp);
    (temps[temps.len() / 2], temps[0])
}

fn main() {
    let filtre: Option<String> = std::env::args().skip(1).find(|a| !a.starts_with('-'));
    let depot = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut scenarios: Vec<(String, String)> = vec![
        ("maximum_10000".into(), {
            let mut alea = chem::alea::Alea::new(1);
            let valeurs: Vec<String> = (0..10_000)
                .map(|_| alea.entre(-1_000_000, 1_000_000).to_string())
                .collect();
            format!(
                "let max = replace x, y by x if x >= y ; {{ {} | max }}",
                valeurs.join(", ")
            )
        }),
        (
            "crible_1000".into(),
            "let c = replace x, y by x if y mod x == 0 ; { 2..1000 | c }".into(),
        ),
    ];
    let mut fichiers: Vec<_> = std::fs::read_dir(depot.join("exemples/performance"))
        .expect("exemples/performance")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "chem"))
        .collect();
    fichiers.sort();
    for f in fichiers {
        let nom = f.file_stem().unwrap().to_string_lossy().to_string();
        scenarios.push((nom, charger(&f)));
    }
    let coeurs = std::thread::available_parallelism().map_or(1, |n| n.get());
    println!("# Mesures du moteur ({REPETITIONS} repetitions, {coeurs} coeur(s) disponible(s))\n");
    println!("| scenario | workers | mediane (ms) | min (ms) | acceleration | efficacite |");
    println!("|---|---:|---:|---:|---:|---:|");
    for (nom, texte) in &scenarios {
        if filtre.as_ref().is_some_and(|f| !nom.contains(f.as_str())) {
            continue;
        }
        let mut reference = 0.0;
        for workers in [1, 2, 4] {
            let (mediane, minimum) = serie(texte, workers, false);
            if workers == 1 {
                reference = mediane;
            }
            let acceleration = reference / mediane;
            println!(
                "| {nom} | {workers} | {mediane:.2} | {minimum:.2} | {acceleration:.2} | {:.2} |",
                acceleration / workers as f64
            );
        }
    }
    if filtre.as_ref().is_none_or(|f| "trace".contains(f.as_str())) {
        println!("\n## Surcout de la trace JSON Lines (EF-V7, 1 worker)\n");
        println!("| scenario | sans trace (ms) | avec trace (ms) | surcout |");
        println!("|---|---:|---:|---:|");
        for (nom, texte) in scenarios
            .iter()
            .filter(|(n, _)| n.starts_with("crible_1000") || n.starts_with("maximum_10000"))
        {
            let (sans, _) = serie(texte, 1, false);
            let (avec, _) = serie(texte, 1, true);
            println!(
                "| {nom} | {sans:.2} | {avec:.2} | {:+.0} % |",
                (avec / sans - 1.0) * 100.0
            );
        }
    }
}
