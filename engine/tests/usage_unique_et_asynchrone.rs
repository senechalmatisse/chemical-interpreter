//! Regles a usage unique et regles sans reactif, injection
//! asynchrone : semantique, concurrence, conservation des molecules
//! et verification des traces par re-execution.

mod commun;

use chem::Interpreteur;
use chem::moteur::Arret;
use chem::ordonnanceur::{Configuration, Issue, Strategie};
use chem::rejeu;
use chem::solution::Solution;
use chem::source::Source;
use chem::trace::{Diffuseur, Enregistreur, Evenement};
use commun::*;
use std::sync::Arc;
use std::time::Duration;

/// Somme de toutes les valeurs entieres, sous-solutions comprises.
fn somme(s: &Solution) -> i64 {
    s.valeurs()
        .iter()
        .filter_map(|v| v.comme_entier())
        .sum::<i64>()
        + s.sous_solutions().map(somme).sum::<i64>()
}

fn entiers_directs(s: &Solution) -> Vec<i64> {
    s.valeurs()
        .iter()
        .filter_map(|v| v.comme_entier())
        .collect()
}

#[test]
fn usage_unique_une_seule_fois_meme_reparti_en_lots() {
    // 1000 molecules : plusieurs lots reagissent en parallele, mais le
    // drapeau de la regle est partage : une seule molecule est negativee.
    let texte = "let marquer = replace-one x:int by -x if x > 0 ;\n{ 1..1000 | marquer }";
    for workers in [1, 2, 4] {
        for graine in 0..15 {
            let (e, _) =
                executer_et_verifier(texte, &config(Strategie::Aleatoire, graine, workers));
            assert!(e.est_inerte());
            let negatifs = entiers_directs(&e.solution)
                .into_iter()
                .filter(|&v| v < 0)
                .count();
            assert_eq!(negatifs, 1, "graine {graine}, {workers} workers");
            assert!(e.solution.regles().is_empty(), "la regle est retiree");
            assert_eq!(e.statistiques.compteurs.regles_retirees, 1);
            assert_eq!(e.statistiques.compteurs.reactions, 1);
        }
    }
}

#[test]
fn chaque_activation_est_un_usage() {
    let texte = "let doubler = replace-one x by 2 * x ;\nlet garder = replace-one x by x ;\n{ 3 | doubler, doubler, garder }";
    for graine in 0..10 {
        let (e, _) = executer_et_verifier(texte, &config(Strategie::Aleatoire, graine, 2));
        assert_eq!(e.solution.to_string(), "{ 12 }");
        assert_eq!(e.statistiques.compteurs.regles_retirees, 3);
    }
}

#[test]
fn regle_sans_reactif_dans_une_solution_vide_ou_repartie() {
    let (e, _) = executer_et_verifier(
        "let amorcer = replace-one by 1, 2 ;\n{ | amorcer }",
        &config(Strategie::Ordonnee, 0, 1),
    );
    assert_eq!(e.solution.to_string(), "{ 1, 2 }");
    // Dans une grande solution repartie en lots, une seule molecule 0.
    let texte = "let amorcer = replace-one by 0 ;\n{ 1..1000 | amorcer }";
    for graine in 0..10 {
        let (e, _) = executer_et_verifier(texte, &config(Strategie::Aleatoire, graine, 4));
        let zeros = entiers_directs(&e.solution)
            .into_iter()
            .filter(|&v| v == 0)
            .count();
        assert_eq!(zeros, 1);
        assert_eq!(e.solution.nombre_molecules(), 1001);
    }
    // Condition constante fausse : jamais applicable, la solution est inerte.
    let e = executer(
        "let jamais = replace by 1 if false ;\n{ | jamais }",
        &config(Strategie::Aleatoire, 1, 1),
    );
    assert!(e.est_inerte());
    assert_eq!(e.statistiques.compteurs.reactions, 0);
}

#[test]
fn regle_sans_reactif_a_usage_multiple_arretee_par_garde_fou() {
    let c = Configuration::constructeur()
        .workers(2)
        .graine(1)
        .max_etapes(Some(500))
        .construire();
    let e = avec_delai(Duration::from_secs(30), move || {
        executer("let a = replace by 1 ;\n{ | a }", &c)
    });
    assert!(matches!(e.issue, Issue::Arretee(Arret::MaxEtapes(500))));
    assert_eq!(e.solution.nombre_molecules(), 500);
}

#[test]
fn references_distinctes() {
    let dupliquer = "let dupliquer = replace x:int, s:ref, t:ref by inject(x, s), inject(x, t) ;\n";
    // Une seule sous-solution : deux references distinctes sont impossibles.
    let e = executer(
        &format!("{dupliquer}{{ 1, 2, 3, {{ }} | dupliquer }}"),
        &config(Strategie::Aleatoire, 1, 2),
    );
    assert!(e.est_inerte());
    assert_eq!(e.solution.to_string(), "{ 1, 2, 3, { } | dupliquer }");
    // Deux sous-solutions : chaque entier est envoye aux deux.
    for graine in 0..10 {
        let (e, _) = executer_et_verifier(
            &format!("{dupliquer}{{ 1, 2, 3, {{ }}, {{ }} | dupliquer }}"),
            &config(Strategie::Aleatoire, graine, 4),
        );
        assert_eq!(
            e.solution.to_string(),
            "{ { 1, 2, 3 }, { 1, 2, 3 } | dupliquer }"
        );
        assert_eq!(e.statistiques.compteurs.envois, 6);
    }
}

#[test]
fn reference_distincte_des_sous_solutions_liees() {
    let m = "let m = replace s:sol, t:ref by dissolve(s), inject(0, t) ;\n";
    let e = executer(
        &format!("{m}{{ {{ 5 }} | m }}"),
        &config(Strategie::Aleatoire, 1, 1),
    );
    assert_eq!(e.solution.to_string(), "{ { 5 } | m }");
    for graine in 0..10 {
        let (e, _) = executer_et_verifier(
            &format!("{m}{{ {{ 5 }}, {{ 7 }} | m }}"),
            &config(Strategie::Aleatoire, graine, 2),
        );
        let texte = e.solution.to_string();
        assert!(
            texte == "{ 5, { 0, 7 } | m }" || texte == "{ 7, { 0, 5 } | m }",
            "{texte}"
        );
    }
}

#[test]
fn envois_et_dissolutions_concurrents_conservent_les_molecules() {
    // 'transmettre' envoie aux sous-solutions pendant que 'vider' les
    // dissout : une sous-solution qui a du courrier n'est jamais liee, une
    // sous-solution dissoute ne recoit plus rien, rien n'est perdu.
    let texte = "let somme = replace x, y by x + y ;\nlet transmettre = replace x:int, s:ref by inject(x, s) ;\nlet vider = replace s:sol by dissolve(s) ;\n{ 1..600, { | somme }, { | somme }, { | somme }, { | somme } | transmettre, vider }";
    avec_delai(Duration::from_secs(120), move || {
        for (graine, workers) in (0..24).map(|g| (g, [1, 2, 4][g as usize % 3])) {
            let strategie = if graine % 4 == 3 {
                Strategie::Ordonnee
            } else {
                Strategie::Aleatoire
            };
            let (e, _) = executer_et_verifier(texte, &config(strategie, graine, workers));
            assert!(e.est_inerte(), "graine {graine} : {:?}", e.issue);
            assert_eq!(e.solution.sous_solutions().count(), 0);
            assert_eq!(somme(&e.solution), 600 * 601 / 2);
            let c = &e.statistiques.compteurs;
            assert_eq!(c.envois, c.receptions);
            assert_eq!(c.dissolutions, 4);
        }
    });
}

#[test]
fn arret_sans_perte_du_courrier_en_transit() {
    // Envois sans fin (W0303) arretes par --max-etapes : le courrier en
    // transit est remis, la somme des valeurs egale le nombre d'envois.
    let texte = "let t = replace s:ref by inject(1, s) ;\nlet somme = replace x, y by x + y ;\n{ { | somme }, { | somme } | t }";
    for workers in [1, 2, 4] {
        let c = Configuration::constructeur()
            .workers(workers)
            .graine(3)
            .max_etapes(Some(3000))
            .delai_max(Some(Duration::from_secs(60)))
            .construire();
        let (e, evenements) = executer_et_verifier(texte, &c);
        assert!(matches!(e.issue, Issue::Arretee(Arret::MaxEtapes(3000))));
        let compteurs = &e.statistiques.compteurs;
        assert!(compteurs.envois > 0);
        assert_eq!(compteurs.receptions, compteurs.envois);
        assert_eq!(somme(&e.solution) as u64, compteurs.envois);
        assert_eq!(e.solution.courrier_en_attente(), 0);
        assert!(matches!(
            evenements.last(),
            Some(Evenement::Fin { inerte: false, .. })
        ));
    }
}

#[test]
fn courrier_recu_pendant_l_execution_de_la_cible() {
    // La sous-solution crible 2..3000 pendant que la solution principale lui
    // envoie 2..400 : avec plusieurs workers, le courrier est releve en
    // plusieurs fois, entre deux passes de la cible (pas seulement a son
    // tour suivant). Le resultat ne depend pas du minutage ; l'entrelacement,
    // lui, en depend : jusqu'a 40 essais (au moins 6) pour l'observer, ce qui
    // rend le test stable sur une machine chargee (integration continue).
    let texte = "let crible = replace x, y by x if y mod x == 0 ;\nlet transmettre = replace x:int, s:ref by inject(x, s) ;\n{ 2..400, { 2..3000 | crible } | transmettre }";
    let paralleles = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut entrelacement = false;
    for graine in 0..40 {
        if entrelacement && graine >= 6 {
            break;
        }
        let (e, evenements) = executer_et_verifier(texte, &config(Strategie::Aleatoire, graine, 4));
        let sous = e.solution.sous_solutions().next().expect("sous-solution");
        assert_eq!(sous.molecules().count(), 430, "nombres premiers < 3000");
        // Une reaction de la cible precede-t-elle une reception ?
        let cible = sous.id();
        let mut reaction_vue = false;
        for ev in &evenements {
            match ev {
                Evenement::Reaction { sol, .. } if *sol == cible => reaction_vue = true,
                Evenement::Reception { sol, .. } if *sol == cible && reaction_vue => {
                    entrelacement = true;
                }
                _ => {}
            }
        }
    }
    if paralleles >= 2 {
        assert!(
            entrelacement,
            "aucune reception pendant l'execution de la cible en 40 essais"
        );
    }
}

/// Trace d'une execution sequentielle ordonnee, ligne par ligne.
fn trace(
    texte: &str,
) -> (
    Vec<String>,
    std::collections::BTreeMap<String, Arc<chem::moteur::RegleCompilee>>,
) {
    let interpreteur = Interpreteur::new();
    let programme = interpreteur
        .charger(&Source::new("t.chem", texte), None)
        .expect("programme valide");
    let regles = programme.compile.regles.clone();
    let enregistreur = Arc::new(Enregistreur::new());
    let diffuseur = Diffuseur::new().avec(Arc::clone(&enregistreur));
    interpreteur.executer(
        programme,
        &config(Strategie::Ordonnee, 0, 1),
        &diffuseur,
        None,
    );
    let lignes = enregistreur
        .evenements()
        .iter()
        .map(Evenement::vers_ligne)
        .collect();
    (lignes, regles)
}

fn rejouer(
    lignes: &[String],
    regles: &std::collections::BTreeMap<String, Arc<chem::moteur::RegleCompilee>>,
) -> Result<(), String> {
    let texte = lignes.join("\n");
    rejeu::rejouer(texte.as_bytes(), Some(regles))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn falsifier(lignes: &[String], de: &str, vers: &str) -> Vec<String> {
    let mut trouve = false;
    let falsifiees = lignes
        .iter()
        .map(|l| {
            if !trouve && l.contains(de) {
                trouve = true;
                l.replacen(de, vers, 1)
            } else {
                l.clone()
            }
        })
        .collect();
    assert!(trouve, "motif '{de}' absent de la trace");
    falsifiees
}

#[test]
fn traces_falsifiees_refusees() {
    let (lignes, regles) = trace(&exemple("b20").texte);
    rejouer(&lignes, &regles).expect("trace authentique valide");
    // regle a usage unique non retiree
    let e = rejouer(&falsifier(&lignes, ",\"retiree\":true", ""), &regles).unwrap_err();
    assert!(e.contains("n'a pas ete retiree"), "{e}");
    // envoi vers une solution qui n'est pas une sous-solution
    let e = rejouer(
        &falsifier(&lignes, "\"cible\":\"s1\"", "\"cible\":\"s0\""),
        &regles,
    )
    .unwrap_err();
    assert!(e.contains("pas une sous-solution"), "{e}");
    // valeur envoyee differente de celle calculee par la regle
    let e = rejouer(&falsifier(&lignes, "\"v\":50", "\"v\":51"), &regles).unwrap_err();
    assert!(e.contains("non prevu"), "{e}");
    // envoi supprime : la reception porte sur une molecule jamais envoyee
    let sans_envoi: Vec<String> = lignes
        .iter()
        .filter(|l| !l.contains("\"ev\":\"envoi\""))
        .cloned()
        .collect();
    assert!(rejouer(&sans_envoi, &regles).is_err());

    // regle a usage unique appliquee une seconde fois
    let (lignes, regles) = trace(&exemple("b18").texte);
    let amorcer = lignes
        .iter()
        .find(|l| l.contains("\"regle\":\"amorcer\""))
        .unwrap()
        .clone();
    let mut doublee = lignes.clone();
    let position = doublee.iter().position(|l| *l == amorcer).unwrap();
    let seconde = amorcer
        .replace("\"n\":1,", "\"n\":99,")
        .replace("\"id\":1,", "\"id\":901,")
        .replace("\"id\":2,", "\"id\":902,")
        .replace("\"id\":3,", "\"id\":903,");
    doublee.insert(position + 1, seconde);
    let e = rejouer(&doublee, &regles).unwrap_err();
    assert!(e.contains("plus) active"), "{e}");
}
