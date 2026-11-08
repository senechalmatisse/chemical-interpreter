//! Repartition des lots par cle (jointures symetriques) : memes resultats
//! qu'en distribution circulaire, traces verifiees, fusions evitees
//! seulement quand les lots sont independants. Les jointures asymetriques
//! (chaines) sont dans tests/contraction.rs.

mod commun;

use chem::{Configuration, Strategie};
use commun::{config, executer_et_verifier};
use std::time::Duration;

const FUSIONNER: &str = "let fusionner = replace x, y by x if x == y ;\n";
const CUMULER: &str = "let cumuler = replace (k, a), (l, b) by (k, a + b) if k == l ;\n";

fn sans_partition(strategie: Strategie, graine: u64, workers: usize) -> Configuration {
    Configuration::constructeur()
        .strategie(strategie)
        .graine(graine)
        .workers(workers)
        .delai_max(Some(Duration::from_secs(60)))
        .partition(false)
        .construire()
}

/// `n` valeurs entre 0 et `distinctes - 1`, melangees de facon fixe.
fn valeurs(n: usize, distinctes: usize) -> String {
    (0..n)
        .map(|k| ((k * 37 + 11) % distinctes).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `n` couples `(cle, valeur)` sur `cles` cles.
fn couples(n: usize, cles: usize) -> String {
    (0..n)
        .map(|k| format!("({}, {})", (k * 7 + 3) % cles, k % 5))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Programmes confluents : le resultat ne depend ni de la repartition, ni
/// du nombre de workers, ni de la graine.
fn programmes() -> Vec<(&'static str, String)> {
    vec![
        (
            "doublons",
            format!("{FUSIONNER}{{ {} | fusionner }}", valeurs(600, 150)),
        ),
        (
            "regroupement_40_cles",
            format!("{CUMULER}{{ {} | cumuler }}", couples(600, 40)),
        ),
        (
            "regroupement_3_cles",
            format!("{CUMULER}{{ {} | cumuler }}", couples(600, 3)),
        ),
        (
            "deux_regles",
            format!(
                "{CUMULER}let oublier = replace s:str by 1 ;\n{{ {}, {} | cumuler, oublier }}",
                couples(400, 25),
                (0..200)
                    .map(|k| format!("\"m{k}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        (
            // x, x -> x + 1 : les produits changent de cle, les lots ne
            // sont pas independants (la fusion reste necessaire)
            "retenue_binaire",
            format!(
                "let retenue = replace x, y by x + 1 if x == y ;\n{{ {}, {} | retenue }}",
                (1..=300)
                    .map(|k| k.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                (1..=300)
                    .map(|k| k.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        (
            // envoi des doublons dans une sous-solution (reference s:ref)
            "references",
            format!(
                "let ecarter = replace x:int, y:int, s:ref by x, inject(y, s) if x == y ;\n{{ {}, {{ }} | ecarter }}",
                valeurs(400, 100)
            ),
        ),
        (
            "sous_solutions",
            format!(
                "{FUSIONNER}{{ {}, {{ {} | fusionner }}, {{ 4, 4 }} | fusionner }}",
                valeurs(300, 60),
                valeurs(300, 20)
            ),
        ),
    ]
}

#[test]
fn memes_resultats_avec_et_sans_partition() {
    for (nom, texte) in programmes() {
        let (reference, _) =
            executer_et_verifier(&texte, &sans_partition(Strategie::Ordonnee, 0, 1));
        assert!(reference.est_inerte(), "{nom}");
        let attendu = reference.solution.to_string();
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
fn resultats_attendus() {
    let resultat = |texte: &str| {
        executer_et_verifier(texte, &config(Strategie::Aleatoire, 7, 4))
            .0
            .solution
            .to_string()
    };
    let doublons = resultat(&format!(
        "{FUSIONNER}{{ {} | fusionner }}",
        valeurs(600, 150)
    ));
    let attendu: Vec<String> = (0..150).map(|v| v.to_string()).collect();
    assert_eq!(
        doublons,
        format!("{{ {} | fusionner }}", attendu.join(", "))
    );
    // 2 x (2^1 + ... + 2^300) = 2^2 + ... + 2^301
    let programmes = programmes();
    let retenue = resultat(
        &programmes
            .iter()
            .find(|(n, _)| *n == "retenue_binaire")
            .unwrap()
            .1,
    );
    let attendu: Vec<String> = (2..=301).map(|v| v.to_string()).collect();
    assert_eq!(retenue, format!("{{ {} | retenue }}", attendu.join(", ")));
}

#[test]
fn partition_et_fusions_evitees() {
    let compteurs = |texte: &str, configuration: &Configuration| {
        let (e, _) = executer_et_verifier(texte, configuration);
        let c = e.statistiques.compteurs;
        (c.repartitions_par_cle, c.fusions_evitees)
    };
    let quatre = config(Strategie::Aleatoire, 3, 4);
    let programmes = programmes();
    let texte = |nom: &str| &programmes.iter().find(|(n, _)| *n == nom).unwrap().1;

    // une cle par lot, jointure locale, seule regle : aucune fusion
    let (repartitions, evitees) = compteurs(texte("doublons"), &quatre);
    assert!(repartitions > 0 && evitees == repartitions);
    let (repartitions, evitees) = compteurs(texte("regroupement_40_cles"), &quatre);
    assert!(repartitions > 0 && evitees == repartitions);
    // groupes plus grands qu'un lot, deuxieme regle, produits de cle
    // nouvelle, reference : repartition par cle, mais fusion
    for nom in [
        "regroupement_3_cles",
        "deux_regles",
        "retenue_binaire",
        "references",
    ] {
        let (repartitions, evitees) = compteurs(texte(nom), &quatre);
        assert!(repartitions > 0, "{nom}");
        assert_eq!(evitees, 0, "{nom}");
    }
    // un seul worker, partition desactivee (les jointures asymetriques sont
    // traitees dans tests/contraction.rs)
    assert_eq!(
        compteurs(texte("doublons"), &config(Strategie::Aleatoire, 3, 1)),
        (0, 0)
    );
    assert_eq!(
        compteurs(
            texte("doublons"),
            &sans_partition(Strategie::Aleatoire, 3, 4)
        ),
        (0, 0)
    );
    // usage unique : une seule reaction, jamais de lots independants
    let unique = format!(
        "let une = replace-one x, y by x if x == y ;\n{{ {} }}",
        valeurs(400, 100)
    )
    .replace(" }", " | une }");
    let (e, _) = executer_et_verifier(&unique, &quatre);
    assert_eq!(e.statistiques.compteurs.reactions, 1);
    assert_eq!(e.statistiques.compteurs.fusions_evitees, 0);
}

#[test]
fn courrier_recu_pendant_une_phase_repartie() {
    // La cible deduplique pendant que sa parente lui envoie des valeurs :
    // le courrier peut arriver dans n'importe quel lot, la fusion est alors
    // conservee. Le resultat ne depend pas du minutage.
    let texte = format!(
        "{FUSIONNER}let transmettre = replace x:int, s:ref by inject(x, s) ;\n{{ {}, {{ {} | fusionner }} | transmettre }}",
        valeurs(300, 90),
        valeurs(600, 120)
    );
    let attendu: Vec<String> = (0..120).map(|v| v.to_string()).collect();
    let attendu = format!(
        "{{ {{ {} | fusionner }} | transmettre }}",
        attendu.join(", ")
    );
    for graine in 0..6 {
        let (e, _) = executer_et_verifier(&texte, &config(Strategie::Aleatoire, graine, 4));
        assert!(e.est_inerte());
        assert_eq!(e.solution.to_string(), attendu, "graine {graine}");
    }
}
