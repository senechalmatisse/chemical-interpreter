//! Programmes invalides : chacun produit une erreur localisee, sans plantage

mod commun;

use chem::Interpreteur;
use chem::source::Source;

#[test]
fn chaque_programme_invalide_est_rejete_avec_une_erreur_localisee() {
    let repertoire = commun::depot().join("engine/tests/invalides");
    let mut fichiers: Vec<_> = std::fs::read_dir(&repertoire)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "chem"))
        .collect();
    fichiers.sort();
    assert!(fichiers.len() >= 10, "au moins 10 programmes invalides");
    for chemin in fichiers {
        let texte = std::fs::read_to_string(&chemin).unwrap();
        let entete = texte
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("# erreur : "))
            .unwrap_or_else(|| {
                panic!(
                    "{} : en-tete '# erreur : CODE L:C' manquant",
                    chemin.display()
                )
            });
        let (code, position) = entete.split_once(' ').unwrap();
        let source = Source::new(chemin.display().to_string(), texte.clone());
        let diagnostics = Interpreteur::new()
            .charger(&source, Some(1_000_000))
            .expect_err(&format!("{} devrait etre rejete", chemin.display()));
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.code == code)
            .unwrap_or_else(|| {
                panic!("{} : code {code} absent de {diagnostics}", chemin.display())
            });
        let pos = source.position(diagnostic.span().expect("erreur localisee").debut);
        assert_eq!(
            pos.to_string(),
            position,
            "{} : {}",
            chemin.display(),
            diagnostic
        );
        assert!(!diagnostic.message.is_empty());
        // le rendu complet cite le fichier, la ligne et la colonne
        let rendu = chem::diagnostic::rendre(diagnostic, &source);
        assert!(
            rendu.contains(&format!("{}:{position}", chemin.display())),
            "{rendu}"
        );
    }
}

#[test]
fn entrees_arbitraires_sans_plantage() {
    // Fragments plausibles assembles au hasard : l'analyse doit toujours
    // rendre la main (succes ou diagnostic), jamais paniquer.
    let fragments = [
        "let",
        "replace",
        "by",
        "if",
        "and",
        "or",
        "not",
        "mod",
        "x",
        "y",
        "s",
        ":",
        "int",
        "sol",
        ",",
        ";",
        "{",
        "}",
        "(",
        ")",
        "|",
        "..",
        "1",
        "-",
        "+",
        "*",
        "/",
        "==",
        "<",
        ">=",
        "!=",
        "dissolve",
        "inject",
        "\"a\"",
        "true",
        "#c\n",
        "=",
        "@",
        "99999999999999999999",
        "\n",
        "\u{e9}",
    ];
    commun::pour_tout("analyse robuste", 3000, 11, |alea| {
        let n = alea.indice(40);
        let texte: Vec<&str> = (0..n)
            .map(|_| fragments[alea.indice(fragments.len())])
            .collect();
        let texte = texte.join(" ");
        let resultat = std::panic::catch_unwind(|| {
            let _ =
                Interpreteur::new().charger(&Source::new("aleatoire", texte.clone()), Some(10_000));
        });
        resultat.map_err(|_| format!("panique sur : {texte:?}"))
    });
    commun::pour_tout("octets arbitraires", 2000, 12, |alea| {
        let n = alea.indice(60);
        let octets: Vec<u8> = (0..n).map(|_| alea.indice(128) as u8).collect();
        let texte = String::from_utf8_lossy(&octets).to_string();
        std::panic::catch_unwind(|| {
            let _ =
                Interpreteur::new().charger(&Source::new("octets", texte.clone()), Some(10_000));
        })
        .map_err(|_| format!("panique sur : {texte:?}"))
    });
}
