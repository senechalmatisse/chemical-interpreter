//! Format canonique d'une solution, partage par le moteur, le rejeu de trace
//! et les tests : deux solutions de meme contenu ont le meme texte.

use crate::evaluateur::Valeur;

/// Formate `{ valeurs triees, sous-solutions triees | regles }`.
///
/// Les sous-solutions sont fournies deja formatees ; l'ordre des regles est
/// conserve (il est significatif pour la strategie ordonnee).
pub fn formater_canonique(
    mut valeurs: Vec<&Valeur>,
    mut sous_solutions: Vec<String>,
    regles: &[&str],
) -> String {
    valeurs.sort();
    sous_solutions.sort();
    let mut morceaux: Vec<String> = valeurs.iter().map(|v| v.to_string()).collect();
    morceaux.extend(sous_solutions);
    let mut texte = String::from("{");
    if !morceaux.is_empty() {
        texte.push(' ');
        texte.push_str(&morceaux.join(", "));
    }
    if !regles.is_empty() {
        texte.push_str(" | ");
        texte.push_str(&regles.join(", "));
    }
    texte.push_str(" }");
    texte
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formes() {
        assert_eq!(formater_canonique(vec![], vec![], &[]), "{ }");
        assert_eq!(formater_canonique(vec![], vec![], &["max"]), "{ | max }");
        let v = [Valeur::Entier(9), Valeur::Entier(2)];
        assert_eq!(
            formater_canonique(v.iter().collect(), vec!["{ 1 }".into()], &["a", "b"]),
            "{ 2, 9, { 1 } | a, b }"
        );
    }
}
