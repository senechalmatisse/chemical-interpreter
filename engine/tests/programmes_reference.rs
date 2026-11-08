//! Programmes de reference (annexe B) et criteres de recette des jalons.

mod commun;

use chem::ordonnanceur::Strategie;
use chem::trace::Evenement;
use commun::*;

#[test]
fn tous_les_exemples_donnent_le_resultat_attendu() {
    let exemples = exemples();
    assert!(exemples.len() >= 8, "au moins 8 programmes d'exemple");
    let mut verifies = 0;
    for e in &exemples {
        let Some(attendu) = &e.attendu else { continue };
        let mut configurations = vec![config(Strategie::Ordonnee, 0, 1)];
        for graine in 1..=6 {
            for workers in [1, 2, 4] {
                configurations.push(config(Strategie::Aleatoire, graine, workers));
            }
        }
        for c in &configurations {
            let execution = executer(&e.texte, c);
            assert!(
                execution.est_inerte(),
                "{} : {:?}",
                e.chemin.display(),
                execution.issue
            );
            assert_eq!(
                &execution.solution.to_string(),
                attendu,
                "{} avec {:?}",
                e.chemin.display(),
                c
            );
        }
        verifies += 1;
    }
    assert!(verifies >= 8);
}

#[test]
fn recette_jalon_1() {
    let c = config(Strategie::Ordonnee, 0, 1);
    assert_eq!(
        executer(&exemple("b01").texte, &c).solution.to_string(),
        "{ 17 | max }"
    );
    assert_eq!(
        executer(&exemple("b10").texte, &c).solution.to_string(),
        "{ 17, 17 | max_strict }"
    );
    assert_eq!(
        executer(&exemple("b02").texte, &c).solution.to_string(),
        "{ 5050 | somme }"
    );
    assert_eq!(
        executer(&exemple("b03").texte, &c).solution.to_string(),
        "{ 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47 | crible }"
    );
    assert_eq!(
        executer(&exemple("b04").texte, &c).solution.to_string(),
        "{ 6 | soustraire, fusionner }"
    );
}

#[test]
fn recette_jalon_2_et_3() {
    for workers in [1, 2, 4] {
        let c = config(Strategie::Aleatoire, 7, workers);
        assert_eq!(
            executer(&exemple("b08").texte, &c).solution.to_string(),
            "{ { 17 | max }, { 2 | min } }"
        );
        assert_eq!(
            executer(&exemple("b06").texte, &c).solution.to_string(),
            "{ 21 | vider, max }"
        );
        assert_eq!(
            executer(&exemple("b07").texte, &c).solution.to_string(),
            "{ { 25 | max } | envoyer }"
        );
    }
}

fn sans_horodatage(evenements: Vec<Evenement>) -> Vec<String> {
    evenements
        .into_iter()
        .map(|e| {
            let mut json = e.vers_json();
            if let chem::trace::json::Json::Objet(champs) = &mut json {
                champs.retain(|(k, _)| k != "t");
            }
            json.to_string()
        })
        .collect()
}

#[test]
fn meme_graine_meme_execution() {
    for prefixe in [
        "b01", "b04", "b05", "b06", "b07", "b13", "b17", "b18", "b19", "b20",
    ] {
        let texte = exemple(prefixe).texte;
        for graine in [1, 42, 2026] {
            let c = config(Strategie::Aleatoire, graine, 1);
            let (_, a) = executer_et_verifier(&texte, &c);
            let (_, b) = executer_et_verifier(&texte, &c);
            assert_eq!(
                sans_horodatage(a),
                sans_horodatage(b),
                "{prefixe}, graine {graine}"
            );
        }
    }
}

#[test]
fn graines_differentes_executions_differentes_resultat_identique() {
    let texte = exemple("b03").texte;
    let mut traces = std::collections::HashSet::new();
    for graine in 0..10 {
        let (execution, evenements) =
            executer_et_verifier(&texte, &config(Strategie::Aleatoire, graine, 1));
        assert_eq!(
            execution.solution.to_string(),
            "{ 2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47 | crible }"
        );
        traces.insert(sans_horodatage(evenements));
    }
    assert!(
        traces.len() > 1,
        "la strategie aleatoire doit varier l'ordre des reactions"
    );
}

#[test]
fn le_parallelisme_est_deterministe_a_graine_fixee() {
    // Les lots ont des graines derivees : le resultat d'un programme non
    // confluent ne depend que de la graine et du nombre de workers.
    let texte = format!(
        "let d = replace x, y by x - y ; {{ {} | d }}",
        liste(&(1..=300).collect::<Vec<_>>())
    );
    for workers in [2, 4] {
        let c = config(Strategie::Aleatoire, 99, workers);
        let premiere = executer(&texte, &c).solution.to_string();
        for _ in 0..5 {
            assert_eq!(executer(&texte, &c).solution.to_string(), premiere);
        }
    }
}

#[test]
fn trace_conforme_a_l_annexe_c() {
    let (_, evenements) =
        executer_et_verifier(&exemple("b01").texte, &config(Strategie::Ordonnee, 0, 1));
    let lignes: Vec<String> = evenements.iter().map(Evenement::vers_ligne).collect();
    assert!(lignes[0].contains("\"ev\":\"debut\""));
    assert!(lignes[1].contains(
        "\"ev\":\"init\",\"sol\":\"s0\",\"parent\":null,\"regles\":[\"max\"],\"mol\":[{\"id\":1,\"v\":4},{\"id\":2,\"v\":17},{\"id\":3,\"v\":9},{\"id\":4,\"v\":17},{\"id\":5,\"v\":2}]"
    ));
    assert!(lignes[2].contains("\"ev\":\"reaction\",\"n\":1,\"w\":0,\"sol\":\"s0\",\"regle\":\"max\",\"in\":[2,1],\"out\":[{\"id\":6,\"v\":17}]"));
    assert_eq!(
        lignes
            .iter()
            .filter(|l| l.contains("\"ev\":\"reaction\""))
            .count(),
        4
    );
    assert!(
        lignes
            .last()
            .unwrap()
            .contains("\"ev\":\"fin\",\"reactions\":4,\"inerte\":true,\"motif\":\"inertie\"")
    );
}

#[test]
fn inertie_detectee_dans_les_cas_limites() {
    let c = config(Strategie::Aleatoire, 3, 4);
    assert_eq!(executer("{ }", &c).solution.to_string(), "{ }");
    assert_eq!(
        executer("let r = replace x by x + 1 if x < 0; { 1, 2 | r }", &c)
            .solution
            .to_string(),
        "{ 1, 2 | r }"
    );
    assert_eq!(
        executer("let r = replace x, y by x; { 7 | r }", &c)
            .solution
            .to_string(),
        "{ 7 | r }"
    );
    assert_eq!(
        executer("let r = replace x by; { 1, 2, 3 | r }", &c)
            .solution
            .to_string(),
        "{ | r }"
    );
    // une sous-solution vide et sans regle est immediatement inerte puis dissoute
    let e = executer(
        "let v = replace s:sol by dissolve(s); { { }, { { } } | v }",
        &c,
    );
    assert_eq!(e.solution.to_string(), "{ | v }");
    // conservation : la sous-solution reste, les entiers sont consommes
    let e = executer("let g = replace s:sol, x:int by s; { { 1 }, 5, 6 | g }", &c);
    assert_eq!(e.solution.to_string(), "{ { 1 } | g }");
    // destruction : une sous-solution liee et non reproduite disparait
    let e = executer("let d = replace s:sol by 0; { { 1, 2 }, { 3 } | d }", &c);
    assert_eq!(e.solution.to_string(), "{ 0, 0 | d }");
}

#[test]
fn statistiques_coherentes() {
    let e = executer(&exemple("b06").texte, &config(Strategie::Ordonnee, 0, 1));
    let s = &e.statistiques;
    assert_eq!(s.compteurs.reactions, 10);
    assert_eq!(s.compteurs.par_regle["vider"], 3);
    assert_eq!(s.compteurs.par_regle["max"], 7);
    assert_eq!(s.compteurs.dissolutions, 3);
    assert_eq!((s.taille_initiale, s.taille_finale), (8, 1));
    assert_eq!((s.solutions_initiales, s.solutions_finales), (4, 1));
    assert_eq!(s.compteurs.par_worker.values().sum::<u64>(), 10);
}
