//! Tests de concurrence : executions repetees
//! avec plusieurs workers, detection d'interblocage par delai, et
//! verification de chaque trace par re-execution (aucune molecule perdue ou
//! dupliquee, aucune sous-solution liee avant son inertie).

mod commun;

use chem::alea::Alea;
use chem::ordonnanceur::Strategie;
use commun::*;
use std::time::Duration;

const REPETITIONS: u64 = 100;

fn repeter(texte: &'static str, attendu: &'static str, workers: usize) {
    avec_delai(Duration::from_secs(120), move || {
        for graine in 0..REPETITIONS {
            let c = config(Strategie::Aleatoire, graine, workers);
            let e = executer(texte, &c);
            assert!(e.est_inerte(), "graine {graine} : {:?}", e.issue);
            assert_eq!(
                e.solution.to_string(),
                attendu,
                "graine {graine}, {workers} workers"
            );
        }
    });
}

#[test]
fn cent_executions_max_min_sans_interblocage() {
    for workers in [1, 2, 4] {
        repeter(
            "let max = replace x, y by x if x >= y ; let min = replace x, y by x if x <= y ;\n{ { 4, 17, 9, 17, 2, 11 | max }, { 4, 17, 9, 17, 2, 11 | min } }",
            "{ { 17 | max }, { 2 | min } }",
            workers,
        );
    }
}

#[test]
fn cent_executions_dissolution() {
    repeter(
        "let max = replace x, y by x if x >= y ; let vider = replace s:sol by dissolve(s) ;\n{ { 4, 17, 9 | max }, { 21, 3 | max }, { 8, 8, 2 | max } | vider, max }",
        "{ 21 | vider, max }",
        4,
    );
    repeter(
        "let somme = replace x, y by x + y ; let vider = replace s:sol by dissolve(s) ;\n{ { { 1, 2 | somme }, 3 | vider, somme }, 4 | vider, somme }",
        "{ 10 | vider, somme }",
        4,
    );
}

#[test]
fn cent_executions_injection() {
    repeter(
        "let max = replace x, y by x if x >= y ; let envoyer = replace x:int, s:sol by inject(x, s) ;\n{ 10, 25, { 4, 17 | max } | envoyer }",
        "{ { 25 | max } | envoyer }",
        4,
    );
}

#[test]
fn cent_executions_injection_asynchrone() {
    // EF-D6 : la sous-solution crible pendant qu'on lui envoie des nombres.
    repeter(
        "let crible = replace x, y by x if y mod x == 0 ; let transmettre = replace x:int, s:ref by inject(x, s) ;\n{ 51..100, { 2..50 | crible } | transmettre }",
        "{ { 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97 | crible } | transmettre }",
        4,
    );
    // EF-D5 + EF-D6 : envoi borne par une regle a usage unique.
    repeter(
        "let max = replace x, y by x if x >= y ; let alimenter = replace-one s:ref by inject(50, s), inject(8, s) ;\n{ { 4, 17, 9 | max } | alimenter }",
        "{ { 50 | max } }",
        4,
    );
}

#[test]
fn traces_paralleles_valides_par_rejeu() {
    // Chaque reaction de chaque trace est re-executee : condition vraie,
    // produits exacts, reactifs presents (pas de double consommation),
    // sous-solutions liees inertes, effets de dissolution/injection/envoi
    // conformes, regles a usage unique retirees une seule fois.
    avec_delai(Duration::from_secs(120), || {
        for graine in 0..30 {
            for prefixe in [
                "b06", "b07", "b08", "b15", "b03", "b05", "b17", "b18", "b19", "b20",
            ] {
                let texte = exemple(prefixe).texte;
                let (e, _) = executer_et_verifier(&texte, &config(Strategie::Aleatoire, graine, 4));
                assert!(e.est_inerte());
            }
        }
    });
}

#[test]
fn grande_solution_imbriquee_en_parallele() {
    avec_delai(Duration::from_secs(120), || {
        let mut alea = Alea::new(5);
        let mut maximum = i64::MIN;
        let mut sous = Vec::new();
        for _ in 0..16 {
            let valeurs: Vec<i64> = (0..300)
                .map(|_| alea.entre(-1_000_000, 1_000_000))
                .collect();
            maximum = maximum.max(*valeurs.iter().max().unwrap());
            sous.push(format!("{{ {} | max }}", liste(&valeurs)));
        }
        let texte = format!(
            "let max = replace x, y by x if x >= y ; let vider = replace s:sol by dissolve(s) ;\n{{ {} | vider, max }}",
            sous.join(", ")
        );
        for workers in [1, 2, 4, 8] {
            let (e, _) = executer_et_verifier(
                &texte,
                &config(Strategie::Aleatoire, workers as u64, workers),
            );
            assert_eq!(
                e.solution.to_string(),
                format!("{{ {maximum} | vider, max }}")
            );
        }
    });
}

#[test]
fn injections_concurrentes_conservent_les_molecules() {
    avec_delai(Duration::from_secs(120), || {
        for graine in 0..20 {
            let texte = "let max = replace x, y by x if x >= y ; let envoyer = replace x:int, s:sol by inject(x, s) ;\n{ 1..400, { 0 | max }, { 0 | max }, { 0 | max }, { 0 | max } | envoyer }";
            let (e, _) = executer_et_verifier(texte, &config(Strategie::Aleatoire, graine, 4));
            // Les entiers sont tous injectes ; chaque sous-solution garde son maximum.
            assert_eq!(e.solution.molecules().count(), 0);
            let maxima: Vec<i64> = e
                .solution
                .sous_solutions()
                .map(|s| {
                    assert_eq!(s.molecules().count(), 1);
                    s.molecules().next().unwrap().valeur.comme_entier().unwrap()
                })
                .collect();
            assert_eq!(maxima.len(), 4);
            assert!(maxima.contains(&400));
        }
    });
}

#[test]
fn un_worker_equivaut_au_mode_sequentiel() {
    // EF-S7 : avec un seul worker, aucune partition ni aucun thread.
    let texte = exemple("b13").texte;
    let c = config(Strategie::Aleatoire, 8, 1);
    let a = executer(&texte, &c).solution.to_string();
    let b = executer(&texte, &c).solution.to_string();
    assert_eq!(a, b);
    let e = executer(&exemple("b09").texte, &c);
    assert_eq!(
        e.statistiques
            .compteurs
            .par_worker
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![0]
    );
}

#[test]
fn garde_fous_en_parallele() {
    let texte = "let r = replace x by x + 1 ; { 1..1000 | r }";
    let c = chem::Configuration::constructeur()
        .workers(4)
        .graine(1)
        .max_etapes(Some(5000))
        .construire();
    let e = avec_delai(Duration::from_secs(30), move || executer(texte, &c));
    assert!(matches!(
        e.issue,
        chem::Issue::Arretee(chem::moteur::Arret::MaxEtapes(5000))
    ));
    assert!(e.statistiques.compteurs.reactions <= 5000);
    // aucune molecule perdue a l'arret
    assert_eq!(e.solution.nombre_molecules(), 1000);
}
