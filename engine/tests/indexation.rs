//! Indexation des molecules par valeur : memes resultats avec et sans
//! index, memes erreurs, traces valides, et recherche effectivement
//! indexee pour les egalites entre reactifs.

mod commun;

use chem::alea::Alea;
use chem::moteur::Arret;
use chem::ordonnanceur::{Configuration, Issue, Strategie};
use commun::*;
use std::time::Duration;

fn avec_index(c: Configuration, actif: bool) -> Configuration {
    let mut constructeur = Configuration::constructeur()
        .strategie(c.strategie())
        .graine(c.graine())
        .workers(c.workers())
        .indexation(actif);
    constructeur = constructeur
        .max_etapes(c.limites().max_etapes)
        .delai_max(c.limites().delai_max)
        .max_molecules(c.limites().max_molecules);
    constructeur.construire()
}

/// Execute avec et sans index ; retourne les deux solutions finales et le
/// nombre de recherches indexees.
fn comparer(texte: &str, base: Configuration) -> (String, String, u64) {
    let (avec, _) = executer_et_verifier(texte, &avec_index(base.clone(), true));
    let (sans, _) = executer_et_verifier(texte, &avec_index(base, false));
    assert!(avec.est_inerte() && sans.est_inerte());
    assert_eq!(sans.statistiques.compteurs.consultations_index, 0);
    (
        avec.solution.to_string(),
        sans.solution.to_string(),
        avec.statistiques.compteurs.consultations_index,
    )
}

/// Chaine `(0, 1), (1, 2), ...` dans un ordre melange.
fn chaine(longueur: usize, alea: &mut Alea) -> String {
    let mut maillons: Vec<String> = (0..longueur).map(|i| format!("({i}, {})", i + 1)).collect();
    alea.melanger(&mut maillons);
    maillons.join(", ")
}

#[test]
fn memes_resultats_avec_et_sans_index() {
    // (definition, nom de la regle, genre de molecules)
    let programmes = [
        // doublons : une seule occurrence de chaque valeur
        (
            "let fusionner = replace x, y by x if x == y ;",
            "fusionner",
            "doublons",
        ),
        // regroupement par cle : somme par cle
        (
            "let cumuler = replace (k, a), (l, b) by (k, a + b) if k == l ;",
            "cumuler",
            "cles",
        ),
        // jointure de chaine : (a, b), (b, c) -> (a, c)
        (
            "let joindre = replace (a, b), (c, d) by (a, d) if b == c ;",
            "joindre",
            "chaine",
        ),
        // egalite precedee d'une condition sure (variables annotees)
        (
            "let fusionner = replace x:int, y:int by x if x >= 0 and x == y ;",
            "fusionner",
            "doublons",
        ),
    ];
    pour_tout("memes_resultats_avec_et_sans_index", 40, 2026, |alea| {
        let (regle, nom, genre) = programmes[alea.indice(programmes.len())];
        let molecules = match genre {
            "doublons" => liste(&entiers(alea, 60, 0, 12)),
            "cles" => {
                let n = 1 + alea.indice(60);
                (0..n)
                    .map(|_| format!("({}, {})", alea.entre(0, 6), alea.entre(-50, 50)))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            _ => chaine(1 + alea.indice(80), alea),
        };
        let texte = format!("{regle}\n{{ {molecules} | {nom} }}");
        let strategie = if alea.indice(2) == 0 {
            Strategie::Aleatoire
        } else {
            Strategie::Ordonnee
        };
        let base = config(strategie, alea.entre(0, 1 << 30) as u64, 1 + alea.indice(4));
        let (avec, sans, consultations) = comparer(&texte, base);
        if avec != sans {
            return Err(format!("{texte}\navec index : {avec}\nsans index : {sans}"));
        }
        if consultations == 0 && molecules.contains(',') {
            return Err(format!("{texte}\nl'index n'a pas ete consulte"));
        }
        Ok(())
    });
}

#[test]
fn erreurs_identiques_avec_et_sans_index() {
    // La cle de l'ancre deborde : repli sur le parcours complet, la meme
    // erreur est signalee.
    let texte = "let r = replace x, y by x if x + 1 == y ;\n{ 9223372036854775807, 5 | r }";
    for actif in [true, false] {
        let e = executer(texte, &avec_index(config(Strategie::Ordonnee, 0, 1), actif));
        match &e.issue {
            Issue::Arretee(Arret::Erreur(erreur)) => assert_eq!(erreur.diagnostic().code, "E0402"),
            autre => panic!("index {actif} : {autre:?}"),
        }
    }
    // Une condition qui peut echouer avant l'egalite empeche l'indexation :
    // l'erreur (division par zero) est signalee dans les deux cas.
    let texte = "let r = replace x, y by x if 10 / y > 0 and x == y ;\n{ 3, 0, 4 | r }";
    for actif in [true, false] {
        let e = executer(texte, &avec_index(config(Strategie::Ordonnee, 0, 1), actif));
        assert!(
            matches!(&e.issue, Issue::Arretee(Arret::Erreur(_))),
            "index {actif}"
        );
        assert_eq!(e.statistiques.compteurs.consultations_index, 0);
    }
    // Apres l'egalite, une erreur n'est atteinte que pour les couples egaux.
    let texte = "let r = replace x, y by x if x == y and 10 / (x - y) > 0 ;\n{ 3, 4, 5 | r }";
    for actif in [true, false] {
        let e = executer(
            texte,
            &avec_index(config(Strategie::Aleatoire, 7, 2), actif),
        );
        assert!(e.est_inerte(), "index {actif} : aucun couple egal");
    }
}

#[test]
fn grande_jointure_indexee() {
    // 3 000 maillons : chaque recherche ne regarde que le seau de sa cle.
    let mut alea = Alea::new(11);
    let texte = format!(
        "let joindre = replace (a, b), (c, d) by (a, d) if b == c ;\n{{ {} | joindre }}",
        chaine(3000, &mut alea)
    );
    for workers in [1, 4] {
        let texte = texte.clone();
        let e = avec_delai(Duration::from_secs(60), move || {
            executer(&texte, &config(Strategie::Aleatoire, 3, workers))
        });
        assert!(e.est_inerte());
        assert_eq!(e.solution.to_string(), "{ (0, 3000) | joindre }");
        assert_eq!(e.statistiques.compteurs.reactions, 2999);
        assert!(e.statistiques.compteurs.consultations_index > 0);
    }
}

#[test]
fn produits_indexes_dans_la_meme_passe() {
    // Les produits sont ranges dans l'index des leur creation : la chaine se
    // referme en une seule passe avec reaction, plus la passe finale.
    let mut alea = Alea::new(5);
    let texte = format!(
        "let joindre = replace (a, b), (c, d) by (a, d) if b == c ;\n{{ {} | joindre }}",
        chaine(200, &mut alea)
    );
    let e = executer(&texte, &config(Strategie::Ordonnee, 0, 1));
    assert_eq!(e.solution.to_string(), "{ (0, 200) | joindre }");
    assert!(
        e.statistiques.compteurs.passes <= 3,
        "{} passes",
        e.statistiques.compteurs.passes
    );
}
