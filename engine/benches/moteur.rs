//! Mesures de performance : `cargo bench` ou `cargo bench -- <filtre>`.
//!
//! Harnais maison (Criterion n'est pas disponible hors ligne) : une
//! execution d'echauffement puis `REPETITIONS` mesures ; on rapporte la
//! mediane et le minimum du temps d'execution du moteur (hors analyse), puis
//! l'acceleration `S = T1 / Tn` et l'efficacite `E = S / n`.
//! Le surcout de la trace JSON Lines, le gain de l'indexation par valeur
//! (conditions d'egalite) et celui de la repartition des lots par cle
//! (jointures symetriques) sont mesures a part. Les programmes sont dans
//! `commun/scenarios.rs`, partages avec les mesures du paquet
//! `integrations` (pool Rayon face au pool standard).

use chem::trace::{Diffuseur, TraceJsonl};
use chem::{Configuration, Interpreteur, Strategie};
use std::io;
use std::path::Path;
use std::time::Duration;

#[allow(dead_code)]
#[path = "commun/scenarios.rs"]
mod scenarios;

const REPETITIONS: usize = 5;

/// Options d'une mesure.
#[derive(Clone, Copy)]
struct Options {
    workers: usize,
    trace: bool,
    indexation: bool,
    partition: bool,
}

impl Options {
    fn new(workers: usize) -> Self {
        Self {
            workers,
            trace: false,
            indexation: true,
            partition: true,
        }
    }
}

fn mesurer(texte: &str, options: Options) -> Duration {
    let interpreteur = Interpreteur::new();
    let config = Configuration::constructeur()
        .strategie(Strategie::Aleatoire)
        .graine(2026)
        .workers(options.workers)
        .max_molecules(None)
        .indexation(options.indexation)
        .partition(options.partition)
        .construire();
    let source = chem::source::Source::new("bench", texte);
    let programme = interpreteur
        .charger(&source, None)
        .expect("programme valide");
    let diffuseur = if options.trace {
        Diffuseur::new().avec(TraceJsonl::new(Box::new(io::sink())))
    } else {
        Diffuseur::new()
    };
    let execution = interpreteur.executer(programme, &config, &diffuseur, None);
    assert!(execution.est_inerte());
    std::hint::black_box(&execution.solution);
    execution.statistiques.duree
}

/// Mediane et minimum (ms) apres une execution d'echauffement.
fn serie(texte: &str, options: Options) -> (f64, f64) {
    mesurer(texte, options);
    let mut temps: Vec<f64> = (0..REPETITIONS)
        .map(|_| mesurer(texte, options).as_secs_f64() * 1000.0)
        .collect();
    temps.sort_by(f64::total_cmp);
    (temps[temps.len() / 2], temps[0])
}

fn main() {
    let filtre: Option<String> = std::env::args().skip(1).find(|a| !a.starts_with('-'));
    let section = |nom: &str| filtre.as_ref().is_none_or(|f| nom.contains(f.as_str()));
    let depot = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let scenarios = scenarios::scenarios(&depot);
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
            let (mediane, minimum) = serie(texte, Options::new(workers));
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
    if section("index") {
        println!("\n## Indexation par valeur des conditions d'egalite (1 worker)\n");
        println!("| scenario | sans index (ms) | avec index (ms) | gain |");
        println!("|---|---:|---:|---:|");
        let comparaisons = [
            ("doublons_4000".to_string(), scenarios::doublons(4000, 3000)),
            (
                "jointure_chaine_3000".to_string(),
                scenarios::jointure_chaine(3000),
            ),
            (
                "regroupement_20000".to_string(),
                scenarios::regroupement(20_000, 50),
            ),
        ];
        for (nom, texte) in &comparaisons {
            let (sans, _) = serie(
                texte,
                Options {
                    indexation: false,
                    ..Options::new(1)
                },
            );
            let (avec, _) = serie(texte, Options::new(1));
            println!("| {nom} | {sans:.2} | {avec:.2} | x{:.0} |", sans / avec);
        }
    }
    if section("partition") {
        println!("\n## Repartition des lots guidee par les jointures (cles, segments de chaine)\n");
        println!(
            "| scenario | workers | circulaire (ms) | guidee (ms) | gain | acceleration guidee |"
        );
        println!("|---|---:|---:|---:|---:|---:|");
        let comparaisons = [
            (
                "doublons_40000".to_string(),
                scenarios::doublons(40_000, 10_000),
            ),
            (
                "regroupement_40000".to_string(),
                scenarios::regroupement(40_000, 10_000),
            ),
            (
                "regroupement_20000".to_string(),
                scenarios::regroupement(20_000, 50),
            ),
            (
                "jointure_chaine_3000".to_string(),
                scenarios::jointure_chaine(3000),
            ),
            (
                "jointure_chaine_30000".to_string(),
                scenarios::jointure_chaine(30_000),
            ),
        ];
        for (nom, texte) in &comparaisons {
            let (un, _) = serie(texte, Options::new(1));
            for workers in [2, 4] {
                let (circulaire, _) = serie(
                    texte,
                    Options {
                        partition: false,
                        ..Options::new(workers)
                    },
                );
                let (par_cle, _) = serie(texte, Options::new(workers));
                println!(
                    "| {nom} | {workers} | {circulaire:.2} | {par_cle:.2} | x{:.2} | {:.2} |",
                    circulaire / par_cle,
                    un / par_cle
                );
            }
        }
    }
    if section("trace") {
        println!("\n## Surcout de la trace JSON Lines (1 worker)\n");
        println!("| scenario | sans trace (ms) | avec trace (ms) | surcout |");
        println!("|---|---:|---:|---:|");
        for (nom, texte) in scenarios
            .iter()
            .filter(|(n, _)| n.starts_with("crible_1000") || n.starts_with("maximum_10000"))
        {
            let (sans, _) = serie(texte, Options::new(1));
            let (avec, _) = serie(
                texte,
                Options {
                    trace: true,
                    ..Options::new(1)
                },
            );
            println!(
                "| {nom} | {sans:.2} | {avec:.2} | {:+.0} % |",
                (avec / sans - 1.0) * 100.0
            );
        }
    }
}
