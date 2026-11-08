//! Executable `chem` construit avec les composants du paquet (Clap, Miette,
//! Serde et Rayon, selon les fonctionnalites actives).

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut es = chem::cli::EntreesSorties::standard();
    let composants = chem_integrations::composants();
    std::process::exit(chem::cli::principal_avec(&arguments, &mut es, &composants));
}
