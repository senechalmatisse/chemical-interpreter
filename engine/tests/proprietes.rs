//! Tests par proprietes (generateur deterministe maison, voir commun::pour_tout).
//!
//! Programmes confluents : le resultat est celui d'un oracle, quels que
//! soient la strategie, la graine et le nombre de workers. Programmes non
//! confluents : on verifie des invariants.

mod commun;

use chem::alea::Alea;
use chem::ast::SansPositions;
use chem::parser::analyser_programme;
use chem::source::Source;
use commun::*;

fn pgcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { pgcd(b, a % b) }
}

#[test]
fn reductions_confluentes() {
    pour_tout("max, min, somme", 150, 1, |alea| {
        let valeurs = entiers(alea, 80, -1000, 1000);
        let c = configuration_aleatoire(alea);
        let l = liste(&valeurs);
        let cas = [
            (
                "let r = replace x, y by x if x >= y ;",
                valeurs.iter().max().map(|m| vec![*m]),
            ),
            (
                "let r = replace x, y by x if x <= y ;",
                valeurs.iter().min().map(|m| vec![*m]),
            ),
            (
                "let r = replace x, y by x + y ;",
                (!valeurs.is_empty()).then(|| vec![valeurs.iter().sum()]),
            ),
        ];
        for (regle, attendu) in cas {
            let texte = format!("{regle} {{ {l} | r }}");
            let obtenu = executer(&texte, &c).solution.to_string();
            let attendu = format!(
                "{{ {}| r }}",
                attendu.map_or(String::new(), |v| format!("{} ", liste(&v)))
            );
            if obtenu != attendu {
                return Err(format!("{texte} -> {obtenu}, attendu {attendu} ({c:?})"));
            }
        }
        Ok(())
    });
}

#[test]
fn pgcd_par_soustractions() {
    pour_tout("pgcd", 120, 2, |alea| {
        let n = 1 + alea.indice(6);
        let facteur = 1 + alea.indice(12) as i64;
        let valeurs: Vec<i64> = (0..n)
            .map(|_| facteur * (1 + alea.indice(30) as i64))
            .collect();
        let attendu = valeurs.iter().copied().reduce(pgcd).unwrap();
        let texte = format!(
            "let soustraire = replace x, y by x - y, y if x > y ; let fusionner = replace x, y by x if x == y ;\n{{ {} | soustraire, fusionner }}",
            liste(&valeurs)
        );
        let obtenu = executer(&texte, &configuration_aleatoire(alea))
            .solution
            .to_string();
        (obtenu == format!("{{ {attendu} | soustraire, fusionner }}"))
            .then_some(())
            .ok_or(format!("{valeurs:?} -> {obtenu}, attendu {attendu}"))
    });
}

#[test]
fn crible_donne_les_nombres_premiers() {
    pour_tout("crible", 40, 3, |alea| {
        let n = 2 + alea.indice(250) as i64;
        let premiers: Vec<i64> = (2..=n).filter(|&k| (2..k).all(|d| k % d != 0)).collect();
        let texte = format!("let c = replace x, y by x if y mod x == 0 ; {{ 2..{n} | c }}");
        let obtenu = executer(&texte, &configuration_aleatoire(alea))
            .solution
            .to_string();
        (obtenu == format!("{{ {} | c }}", liste(&premiers)))
            .then_some(())
            .ok_or(format!("crible {n} -> {obtenu}"))
    });
}

#[test]
fn tri_par_echanges() {
    pour_tout("tri", 60, 4, |alea| {
        let n = alea.indice(12);
        let mut valeurs: Vec<i64> = (0..n).map(|_| alea.entre(-50, 50)).collect();
        let couples: Vec<String> = valeurs
            .iter()
            .enumerate()
            .map(|(i, v)| format!("({i}, {v})"))
            .collect();
        let texte = format!(
            "let e = replace (i, a), (j, b) by (i, b), (j, a) if i < j and a > b ; {{ {} | e }}",
            couples.join(", ")
        );
        valeurs.sort_unstable();
        // l'affichage canonique trie les n-uplets par indice croissant
        let tries: Vec<String> = valeurs
            .iter()
            .enumerate()
            .map(|(i, v)| format!("({i}, {v})"))
            .collect();
        let attendu = if tries.is_empty() {
            "{ | e }".to_string()
        } else {
            format!("{{ {} | e }}", tries.join(", "))
        };
        let obtenu = executer(&texte, &configuration_aleatoire(alea))
            .solution
            .to_string();
        (obtenu == attendu)
            .then_some(())
            .ok_or(format!("{texte} -> {obtenu}, attendu {attendu}"))
    });
}

#[test]
fn invariant_parite_du_programme_non_confluent() {
    pour_tout("difference", 150, 5, |alea| {
        let mut valeurs = entiers(alea, 60, -100, 100);
        if valeurs.is_empty() {
            valeurs.push(1);
        }
        let somme: i64 = valeurs.iter().sum();
        let texte = format!(
            "let d = replace x, y by x - y ; {{ {} | d }}",
            liste(&valeurs)
        );
        let e = executer(&texte, &configuration_aleatoire(alea));
        let restants: Vec<i64> = e
            .solution
            .molecules()
            .map(|m| m.valeur.comme_entier().unwrap())
            .collect();
        if restants.len() != 1 || (restants[0] - somme).rem_euclid(2) != 0 {
            return Err(format!("{valeurs:?} -> {restants:?} (somme {somme})"));
        }
        Ok(())
    });
}

#[test]
fn invariant_maximum_strict() {
    pour_tout("max strict", 150, 6, |alea| {
        let valeurs = entiers(alea, 40, 0, 6);
        let texte = format!(
            "let m = replace x, y by x if x > y ; {{ {} | m }}",
            liste(&valeurs)
        );
        let e = executer(&texte, &configuration_aleatoire(alea));
        let restants: Vec<i64> = e
            .solution
            .molecules()
            .map(|m| m.valeur.comme_entier().unwrap())
            .collect();
        let max = valeurs.iter().max().copied();
        let multiplicite = valeurs.iter().filter(|v| Some(**v) == max).count();
        if restants.len() != multiplicite || restants.iter().any(|v| Some(*v) != max) {
            return Err(format!("{valeurs:?} -> {restants:?}"));
        }
        Ok(())
    });
}

#[test]
fn dissolution_conserve_la_somme() {
    pour_tout("dissolution", 80, 7, |alea| {
        let mut total = 0;
        let sous: Vec<String> = (0..1 + alea.indice(6))
            .map(|_| {
                let v = entiers(alea, 15, -20, 20);
                total += v.iter().sum::<i64>();
                format!("{{ {} | somme }}", liste(&v))
            })
            .collect();
        let texte = format!(
            "let somme = replace x, y by x + y ; let vider = replace s:sol by dissolve(s) ; {{ 0, {} | vider, somme }}",
            sous.join(", ")
        );
        let (e, _) = executer_et_verifier(&texte, &configuration_aleatoire(alea));
        let obtenu = e.solution.to_string();
        (obtenu == format!("{{ {total} | vider, somme }}"))
            .then_some(())
            .ok_or(obtenu)
    });
}

#[test]
fn rejeu_reconstruit_toujours_la_solution_finale() {
    pour_tout("rejeu", 60, 8, |alea| {
        let mut valeurs = entiers(alea, 50, 1, 60);
        valeurs.push(2);
        let texte = format!(
            "let c = replace x, y by x if y mod x == 0 ; let e = replace x:int, s:sol by inject(x, s) if x > 50 ;\n{{ {}, {{ 1 | c }} | c, e }}",
            liste(&valeurs)
        );
        executer_et_verifier(&texte, &configuration_aleatoire(alea));
        Ok(())
    });
}

/// Generateur de programmes aleatoires syntaxiquement valides.
fn expression(alea: &mut Alea, variables: &[&str], profondeur: usize) -> String {
    if profondeur == 0 || alea.probabilite(0.3) {
        return match alea.indice(5) {
            0 if !variables.is_empty() => variables[alea.indice(variables.len())].to_string(),
            1 => "true".into(),
            2 => "\"t\\\"x\"".into(),
            3 => format!("({}, {})", alea.entre(0, 9), alea.entre(-9, 9)),
            _ => alea.entre(0, 99).to_string(),
        };
    }
    let a = expression(alea, variables, profondeur - 1);
    let b = expression(alea, variables, profondeur - 1);
    match alea.indice(14) {
        0 => format!("-({a})"),
        1 => format!("not ({a})"),
        2 => format!("({a})"),
        k => {
            let op = [
                "+", "-", "*", "/", "mod", "==", "!=", "<", "<=", ">", "and", "or",
            ][k - 2];
            format!("({a}) {op} ({b})")
        }
    }
}

#[test]
fn afficheur_aller_retour() {
    // Toutes les sources d'exemple, puis des programmes generes.
    for e in exemples() {
        let ast = analyser_programme(&Source::new("e", e.texte.clone())).unwrap();
        let relu = analyser_programme(&Source::new("r", ast.to_string())).unwrap();
        assert_eq!(
            ast.sans_positions(),
            relu.sans_positions(),
            "{}",
            e.chemin.display()
        );
    }
    pour_tout("aller-retour", 400, 9, |alea| {
        let variables = ["x", "y", "z"];
        let regle = format!(
            "let r = replace x, (y, z:int), s:sol by {}, dissolve(s) if {} ;",
            expression(alea, &variables, 4),
            expression(alea, &variables, 4)
        );
        let solution = format!(
            "{{ {}, 1..{}, {{ {} | r }} | r }}",
            expression(alea, &[], 3),
            alea.entre(1, 5),
            expression(alea, &[], 2)
        );
        let texte = format!("{regle}\n{solution}");
        let ast = analyser_programme(&Source::new("g", texte.clone()))
            .map_err(|d| format!("{d} : {texte}"))?;
        let affiche = ast.to_string();
        let relu = analyser_programme(&Source::new("a", affiche.clone()))
            .map_err(|d| format!("{d} : {affiche}"))?;
        if ast.sans_positions() != relu.sans_positions() {
            return Err(format!("{texte}\n-> {affiche}"));
        }
        if relu.to_string() != affiche {
            return Err(format!("affichage non idempotent : {affiche}"));
        }
        Ok(())
    });
}
