//! Executable `chem` : delegue a la CLI de la bibliotheque.

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut es = chem::cli::EntreesSorties::standard();
    std::process::exit(chem::cli::principal(&arguments, &mut es));
}
