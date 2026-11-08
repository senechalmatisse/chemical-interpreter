//! Pool Rayon face au pool de la bibliotheque standard :
//! `cargo bench --bench pools` ou `cargo bench --bench pools -- <filtre>`
//! (depuis `integrations/`, fonctionnalite `rayon`).
//!
//! Memes programmes que les mesures du moteur
//! (`engine/benches/commun/scenarios.rs`) et meme protocole : une execution
//! d'echauffement, puis `REPETITIONS` mesures alternees entre les deux
//! pools ; on rapporte la mediane du temps d'execution du moteur (hors
//! analyse, creation des threads comprise). Pour chaque scenario et chaque
//! nombre de workers (1, 2, 4, puis 8 et le nombre de coeurs quand la
//! machine en a plus) : temps des deux pools, rapport Rayon / standard
//! (inferieur a 1 : Rayon plus rapide) et acceleration de chaque pool par
//! rapport a son propre temps a 1 worker.
//!
//! Les ecarts n'ont de sens qu'avec au moins 4 coeurs : en dessous, les
//! workers se partagent les coeurs et la comparaison mesure surtout
//! l'ordonnanceur du systeme.

#[allow(dead_code)]
#[path = "../../engine/benches/commun/scenarios.rs"]
mod scenarios;

use chem::ordonnanceur::{FabriqueParallelisme, ParallelismeStandard};
use chem::trace::Diffuseur;
use chem::{Configuration, Interpreteur, Strategie};
use chem_integrations::parallelisme::FabriqueRayon;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const REPETITIONS: usize = 7;

/// Duree d'une execution du moteur.
fn mesurer(interpreteur: &Interpreteur, texte: &str, workers: usize) -> Duration {
    let configuration = Configuration::constructeur()
        .strategie(Strategie::Aleatoire)
        .graine(2026)
        .workers(workers)
        .max_molecules(None)
        .construire();
    let source = chem::source::Source::new("bench", texte);
    let programme = interpreteur
        .charger(&source, None)
        .expect("programme valide");
    let execution = interpreteur.executer(programme, &configuration, &Diffuseur::new(), None);
    assert!(execution.est_inerte());
    std::hint::black_box(&execution.solution);
    execution.statistiques.duree
}

/// Medianes (ms) des deux pools, mesures alternees apres echauffement.
fn comparer(
    standard: &Interpreteur,
    rayon: &Interpreteur,
    texte: &str,
    workers: usize,
) -> (f64, f64) {
    mesurer(standard, texte, workers);
    mesurer(rayon, texte, workers);
    let mut temps_standard = Vec::with_capacity(REPETITIONS);
    let mut temps_rayon = Vec::with_capacity(REPETITIONS);
    for _ in 0..REPETITIONS {
        temps_standard.push(mesurer(standard, texte, workers).as_secs_f64() * 1000.0);
        temps_rayon.push(mesurer(rayon, texte, workers).as_secs_f64() * 1000.0);
    }
    (mediane(temps_standard), mediane(temps_rayon))
}

fn mediane(mut temps: Vec<f64>) -> f64 {
    temps.sort_by(f64::total_cmp);
    temps[temps.len() / 2]
}

fn interpreteur(fabrique: Arc<dyn FabriqueParallelisme>) -> Interpreteur {
    Interpreteur::new().avec_parallelisme(fabrique)
}

fn main() {
    let filtre: Option<String> = std::env::args().skip(1).find(|a| !a.starts_with('-'));
    let depot = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let coeurs = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut paliers = vec![1, 2, 4];
    for n in [8, coeurs] {
        if n <= coeurs && n > paliers[paliers.len() - 1] {
            paliers.push(n);
        }
    }
    let standard = interpreteur(Arc::new(ParallelismeStandard));
    let rayon = interpreteur(Arc::new(FabriqueRayon));

    println!(
        "# Pool Rayon face au pool standard ({REPETITIONS} repetitions, {coeurs} coeur(s) disponible(s))\n"
    );
    if coeurs < 4 {
        println!(
            "> Moins de 4 coeurs : les workers se partagent les coeurs, ecarts peu representatifs.\n"
        );
    }
    println!(
        "| scenario | workers | standard (ms) | rayon (ms) | rayon / standard | acceleration standard | acceleration rayon |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|");
    // Somme des logarithmes des rapports, par palier (moyenne geometrique).
    let mut rapports: Vec<(f64, usize)> = vec![(0.0, 0); paliers.len()];
    for (nom, texte) in scenarios::scenarios(&depot) {
        if filtre.as_ref().is_some_and(|f| !nom.contains(f.as_str())) {
            continue;
        }
        let mut reference = (0.0, 0.0);
        for (rang, &workers) in paliers.iter().enumerate() {
            let (t_standard, t_rayon) = comparer(&standard, &rayon, &texte, workers);
            if workers == 1 {
                reference = (t_standard, t_rayon);
            }
            let rapport = t_rayon / t_standard;
            rapports[rang].0 += rapport.ln();
            rapports[rang].1 += 1;
            println!(
                "| {nom} | {workers} | {t_standard:.2} | {t_rayon:.2} | {rapport:.2} | {:.2} | {:.2} |",
                reference.0 / t_standard,
                reference.1 / t_rayon
            );
        }
    }
    println!("\n## Rapport Rayon / standard, moyenne geometrique des scenarios\n");
    println!("| workers | rayon / standard |");
    println!("|---:|---:|");
    for (&workers, &(somme, n)) in paliers.iter().zip(&rapports) {
        if n > 0 {
            println!("| {workers} | {:.2} |", (somme / n as f64).exp());
        }
    }
}
