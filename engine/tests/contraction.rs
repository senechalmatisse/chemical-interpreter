//! Contraction parallele des chaines (jointures asymetriques) : les
//! maillons sont coupes en segments places chacun dans un lot. Memes
//! resultats qu'en distribution circulaire, traces verifiees, cycles et
//! branches sans perte, longue chaine decoupee par les workers.

mod commun;

use chem::alea::Alea;
use chem::{Configuration, Strategie};
use commun::{config, executer, executer_et_verifier};
use std::time::Duration;

const JOINDRE: &str = "let joindre = replace (a, b), (c, d) by (a, d) if b == c ;\n";

fn sans_partition(strategie: Strategie, graine: u64, workers: usize) -> Configuration {
    Configuration::constructeur()
        .strategie(strategie)
        .graine(graine)
        .workers(workers)
        .delai_max(Some(Duration::from_secs(60)))
        .partition(false)
        .construire()
}

/// Maillons `(debut + i, debut + i + 1)` pour `i` de 0 a `longueur - 1`.
fn maillons(debut: i64, longueur: i64) -> Vec<String> {
    (0..longueur)
        .map(|i| format!("({}, {})", debut + i, debut + i + 1))
        .collect()
}

fn melanges(mut elements: Vec<String>, graine: u64) -> String {
    Alea::new(graine).melanger(&mut elements);
    elements.join(", ")
}

/// Programmes confluents : le resultat ne depend ni de la repartition, ni
/// du nombre de workers, ni de la graine.
fn programmes() -> Vec<(&'static str, String, String)> {
    let plusieurs: Vec<String> = (0..20).flat_map(|c| maillons(c * 1000, 100)).collect();
    let fins: Vec<String> = (0..20)
        .map(|c| format!("({}, {})", c * 1000, c * 1000 + 100))
        .collect();
    let mut sommes = Vec::new();
    let mut attendu_sommes = Vec::new();
    for k in 0..50 {
        sommes.push(format!("({k}, {k})"));
        let mut total = k;
        for w in 0..10 {
            sommes.push(format!("({k}, {w}, \"x\")"));
            total += w;
        }
        attendu_sommes.push(format!("({k}, {total})"));
    }
    vec![
        (
            "chaine_2000",
            format!(
                "{JOINDRE}{{ {} | joindre }}",
                melanges(maillons(0, 2000), 1)
            ),
            "{ (0, 2000) | joindre }".to_string(),
        ),
        (
            "vingt_chaines",
            format!("{JOINDRE}{{ {} | joindre }}", melanges(plusieurs, 2)),
            format!("{{ {} | joindre }}", fins.join(", ")),
        ),
        (
            // formes distinctes : chaque molecule ne tient qu'un role
            "jointure_de_formes",
            format!(
                "let ajouter = replace (k, v), (l, w, x) by (k, v + w) if k == l ;\n{{ {} | ajouter }}",
                melanges(sommes, 3)
            ),
            format!("{{ {} | ajouter }}", attendu_sommes.join(", ")),
        ),
        (
            "chaine_et_autre_regle",
            format!(
                "{JOINDRE}let compter = replace s:str by 1 ;\n{{ {}, {} | joindre, compter }}",
                melanges(maillons(0, 600), 4),
                (0..100)
                    .map(|k| format!("\"m{k}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            format!(
                "{{ {}, (0, 600) | joindre, compter }}",
                vec!["1"; 100].join(", ")
            ),
        ),
    ]
}

#[test]
fn memes_resultats_avec_et_sans_contraction() {
    for (nom, texte, attendu) in programmes() {
        for workers in [1, 2, 4] {
            for graine in 1..=3 {
                for configuration in [
                    config(Strategie::Aleatoire, graine, workers),
                    sans_partition(Strategie::Aleatoire, graine, workers),
                    config(Strategie::Ordonnee, graine, workers),
                ] {
                    let (e, _) = executer_et_verifier(&texte, &configuration);
                    assert!(e.est_inerte(), "{nom}");
                    assert_eq!(
                        e.solution.to_string(),
                        attendu,
                        "{nom}, {workers} worker(s), graine {graine}, partition {}",
                        configuration.partition()
                    );
                }
            }
        }
    }
}

#[test]
fn segments_comptes() {
    let compteurs = |texte: &str, configuration: &Configuration| {
        let (e, _) = executer_et_verifier(texte, configuration);
        let c = e.statistiques.compteurs;
        (
            c.repartitions_par_cle,
            c.segments_de_chaine,
            c.fusions_evitees,
        )
    };
    let chaine = &programmes()[0].1;
    let (repartitions, segments, evitees) = compteurs(chaine, &config(Strategie::Aleatoire, 3, 4));
    assert!(repartitions > 0);
    assert!(segments >= 4, "{segments} segments");
    // des segments a cheval sur deux lots : la fusion reste necessaire
    assert_eq!(evitees, 0);
    assert_eq!(
        compteurs(chaine, &config(Strategie::Aleatoire, 3, 1)),
        (0, 0, 0)
    );
    assert_eq!(
        compteurs(chaine, &sans_partition(Strategie::Aleatoire, 3, 4)),
        (0, 0, 0)
    );
    // formes distinctes : groupes par cle, aucun maillon
    let formes = &programmes()[2].1;
    let (repartitions, segments, _) = compteurs(formes, &config(Strategie::Aleatoire, 3, 4));
    assert!(repartitions > 0);
    assert_eq!(segments, 0);
}

#[test]
fn cycles_et_branches_sans_perte() {
    // Cycle de 300 maillons : le resultat (un couple (x, x)) depend de
    // l'ordre des reactions. Branches : deux maillons pour une meme cle
    // d'ancre, l'un des deux reste. Les traces sont rejouees et verifiees.
    let mut cycle = maillons(0, 299);
    cycle.push("(299, 0)".to_string());
    let cycle = format!("{JOINDRE}{{ {} | joindre }}", melanges(cycle, 5));
    let mut branches = maillons(0, 400);
    branches.extend(
        maillons(1000, 50)
            .into_iter()
            .map(|m| m.replace("(1000,", "(500,")),
    );
    branches.push("(450, 200)".to_string());
    let branches = format!("{JOINDRE}{{ {} | joindre }}", melanges(branches, 6));
    for graine in 0..4 {
        let (e, _) = executer_et_verifier(&cycle, &config(Strategie::Aleatoire, graine, 4));
        assert!(e.est_inerte());
        let texte = e.solution.to_string();
        let interieur = texte
            .trim_start_matches("{ (")
            .trim_end_matches(") | joindre }");
        let (a, b) = interieur.split_once(", ").expect("un seul couple");
        assert_eq!(a, b, "cycle contracte en (x, x) : {texte}");
        let (e, _) = executer_et_verifier(&branches, &config(Strategie::Aleatoire, graine, 4));
        assert!(e.est_inerte());
    }
}

#[test]
fn longue_chaine_decoupee_par_les_workers() {
    // Au-dela de 131 072 maillons, les successeurs et les segments sont
    // calcules par les workers de la phase : le resultat ne change pas.
    let longueur = 140_000;
    let texte = format!(
        "{JOINDRE}{{ {} | joindre }}",
        melanges(maillons(0, longueur), 7)
    );
    for workers in [2, 4] {
        let e = executer(&texte, &config(Strategie::Aleatoire, 1, workers));
        assert!(e.est_inerte());
        assert_eq!(
            e.solution.to_string(),
            format!("{{ (0, {longueur}) | joindre }}")
        );
        assert!(
            e.statistiques.compteurs.segments_de_chaine >= 8,
            "{workers} worker(s)"
        );
    }
}
