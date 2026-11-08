//! Rendu textuel d'un diagnostic avec extrait de source souligne.
//!
//! ```text
//! erreur[E0201]: ';' attendu a la fin de la definition, trouve 'let'
//!   --> b1.chem:2:38
//!    |
//!  2 | let max = replace x, y by x if x >= y
//!    |                                      ^ ';' attendu ici
//!    |
//!    = aide : chaque definition se termine par ';'
//! ```

use super::{Diagnostic, Etiquette};
use crate::source::Source;
use std::fmt::Write;

/// Produit le rendu textuel complet d'un diagnostic.
pub fn rendre(diagnostic: &Diagnostic, source: &Source) -> String {
    let mut sortie = String::new();
    // Les ecritures dans une String ne peuvent pas echouer.
    let _ = ecrire(&mut sortie, diagnostic, source);
    sortie
}

fn ecrire(sortie: &mut String, diagnostic: &Diagnostic, source: &Source) -> std::fmt::Result {
    writeln!(sortie, "{diagnostic}")?;
    let Some(principale) = diagnostic.etiquettes.first() else {
        if let Some(aide) = &diagnostic.aide {
            writeln!(sortie, "  = aide : {aide}")?;
        }
        return Ok(());
    };

    let mut etiquettes: Vec<(usize, &Etiquette)> =
        diagnostic.etiquettes.iter().enumerate().collect();
    etiquettes.sort_by_key(|(_, e)| e.span.debut);
    let derniere_ligne = etiquettes
        .iter()
        .map(|(_, e)| source.position(e.span.debut).ligne)
        .max()
        .unwrap_or(1);
    let largeur = derniere_ligne.to_string().len();
    let marge = " ".repeat(largeur);

    let pos = source.position(principale.span.debut);
    writeln!(
        sortie,
        "{marge}--> {}:{}:{}",
        source.nom(),
        pos.ligne,
        pos.colonne
    )?;
    writeln!(sortie, "{marge} |")?;

    let mut ligne_affichee = 0;
    for (rang, etiquette) in etiquettes {
        let debut = source.position(etiquette.span.debut);
        let texte = source.ligne(debut.ligne).replace('\t', " ");
        if debut.ligne != ligne_affichee {
            writeln!(sortie, "{:>largeur$} | {texte}", debut.ligne)?;
            ligne_affichee = debut.ligne;
        }
        let fin = source.position(etiquette.span.fin);
        let longueur_ligne = texte.chars().count();
        let colonne_fin = if fin.ligne == debut.ligne {
            fin.colonne
        } else {
            longueur_ligne + 1
        };
        let nombre = colonne_fin.saturating_sub(debut.colonne).max(1);
        let trait_ = if rang == 0 { "^" } else { "-" };
        writeln!(
            sortie,
            "{marge} | {}{} {}",
            " ".repeat(debut.colonne - 1),
            trait_.repeat(nombre),
            etiquette.message
        )?;
    }
    writeln!(sortie, "{marge} |")?;
    if let Some(aide) = &diagnostic.aide {
        writeln!(sortie, "{marge} = aide : {aide}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Span;

    #[test]
    fn rendu_avec_extrait() {
        let source = Source::new("b1.chem", "let max = replace x, y by x\n{ 1 }");
        let d = Diagnostic::erreur("E0201", "test")
            .etiquette(Span::new(10, 17), "ici")
            .aide("conseil");
        let texte = rendre(&d, &source);
        assert!(texte.contains("erreur[E0201]: test"));
        assert!(texte.contains("--> b1.chem:1:11"));
        assert!(texte.contains("^^^^^^^ ici"));
        assert!(texte.contains("= aide : conseil"));
    }

    #[test]
    fn rendu_en_fin_de_fichier() {
        let source = Source::new("f", "{ 1");
        let d = Diagnostic::erreur("E0201", "x").etiquette(Span::new(3, 3), "ici");
        let texte = rendre(&d, &source);
        assert!(texte.contains("f:1:4"));
        assert!(texte.contains("   ^ ici"));
    }

    #[test]
    fn rendu_sans_etiquette() {
        let source = Source::new("f", "");
        let d = Diagnostic::avertissement("W0001", "rien").aide("a");
        assert_eq!(
            rendre(&d, &source),
            "avertissement[W0001]: rien\n  = aide : a\n"
        );
    }
}
