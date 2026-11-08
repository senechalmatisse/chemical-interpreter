//! Programmes des mesures, partages par les mesures du moteur
//! (`engine/benches/moteur.rs`) et par celles du paquet `integrations`
//! (`integrations/benches/pools.rs` : pool Rayon face au pool standard).
//! Chaque generateur est deterministe (graine fixe).

use chem::alea::Alea;
use std::path::Path;

/// `n` entiers tires au hasard, reduits a leur maximum.
pub fn maximum(n: usize) -> String {
    let mut alea = Alea::new(1);
    let valeurs: Vec<String> = (0..n)
        .map(|_| alea.entre(-1_000_000, 1_000_000).to_string())
        .collect();
    format!(
        "let max = replace x, y by x if x >= y ; {{ {} | max }}",
        valeurs.join(", ")
    )
}

/// Crible d'Eratosthene de 2 a `n`.
pub fn crible(n: usize) -> String {
    format!("let c = replace x, y by x if y mod x == 0 ; {{ 2..{n} | c }}")
}

/// Chaine `(0, 1), (1, 2), ...` melangee, refermee par jointure sur `b == c`
/// (jointure asymetrique).
pub fn jointure_chaine(longueur: usize) -> String {
    let mut alea = Alea::new(3);
    let mut maillons: Vec<String> = (0..longueur).map(|i| format!("({i}, {})", i + 1)).collect();
    alea.melanger(&mut maillons);
    format!(
        "let joindre = replace (a, b), (c, d) by (a, d) if b == c ; {{ {} | joindre }}",
        maillons.join(", ")
    )
}

/// Somme par cle de `n` couples `(cle, valeur)` sur `cles` cles distinctes
/// (jointure symetrique).
pub fn regroupement(n: usize, cles: i64) -> String {
    let mut alea = Alea::new(4);
    let couples: Vec<String> = (0..n)
        .map(|_| format!("({}, {})", alea.entre(0, cles - 1), alea.entre(-100, 100)))
        .collect();
    format!(
        "let cumuler = replace (k, a), (l, b) by (k, a + b) if k == l ; {{ {} | cumuler }}",
        couples.join(", ")
    )
}

/// `n` valeurs parmi `distinctes`, reduites a une occurrence chacune
/// (jointure symetrique).
pub fn doublons(n: usize, distinctes: i64) -> String {
    let mut alea = Alea::new(5);
    let valeurs: Vec<String> = (0..n)
        .map(|_| alea.entre(0, distinctes - 1).to_string())
        .collect();
    format!(
        "let fusionner = replace x, y by x if x == y ; {{ {} | fusionner }}",
        valeurs.join(", ")
    )
}

/// Scenarios des mesures de parallelisme : programmes generes, puis ceux
/// de `exemples/performance` (par nom de fichier).
pub fn scenarios(depot: &Path) -> Vec<(String, String)> {
    let mut scenarios: Vec<(String, String)> = vec![
        ("maximum_10000".into(), maximum(10_000)),
        ("crible_1000".into(), crible(1000)),
        ("jointure_chaine_3000".into(), jointure_chaine(3000)),
        ("jointure_chaine_30000".into(), jointure_chaine(30_000)),
        ("regroupement_20000".into(), regroupement(20_000, 50)),
        ("doublons_40000".into(), doublons(40_000, 10_000)),
        ("regroupement_40000".into(), regroupement(40_000, 10_000)),
    ];
    let mut fichiers: Vec<_> = std::fs::read_dir(depot.join("exemples/performance"))
        .expect("exemples/performance")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "chem"))
        .collect();
    fichiers.sort();
    for f in fichiers {
        let nom = f.file_stem().unwrap().to_string_lossy().to_string();
        let texte = std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("{} : {e}", f.display()));
        scenarios.push((nom, texte));
    }
    scenarios
}
