//! Codage des evenements de trace en lignes JSON.
//!
//! Le format (docs/format-trace.md) est fixe ; sa mise en oeuvre est
//! interchangeable : [`FormatInterne`] utilise le module [`super::json`] de
//! la bibliotheque, le paquet `integrations` en fournit une avec Serde. Les
//! deux produisent les memes lignes et acceptent les memes traces.

use super::Evenement;
use std::io;

/// Ecriture et lecture d'une ligne de trace.
pub trait FormatTrace: Send + Sync {
    /// Nom de l'implementation (affiche par `chem --version`).
    fn nom(&self) -> &'static str;

    /// Ajoute l'evenement a `sortie`, sur une ligne terminee par `\n`.
    fn ecrire(&self, evenement: &Evenement, sortie: &mut Vec<u8>) -> io::Result<()>;

    /// Decode une ligne (sans le saut de ligne).
    fn lire(&self, ligne: &str) -> Result<Evenement, String>;
}

/// Mise en oeuvre par le module JSON de la bibliotheque.
#[derive(Debug, Default, Clone, Copy)]
pub struct FormatInterne;

impl FormatTrace for FormatInterne {
    fn nom(&self) -> &'static str {
        "interne"
    }

    fn ecrire(&self, evenement: &Evenement, sortie: &mut Vec<u8>) -> io::Result<()> {
        let mut ligne = String::with_capacity(96);
        evenement.vers_json().ecrire(&mut ligne);
        ligne.push('\n');
        sortie.extend_from_slice(ligne.as_bytes());
        Ok(())
    }

    fn lire(&self, ligne: &str) -> Result<Evenement, String> {
        Evenement::depuis_ligne(ligne)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solution::IdSolution;

    #[test]
    fn aller_retour() {
        let e = Evenement::Inertie {
            t: 0.5,
            sol: IdSolution(2),
        };
        let mut sortie = Vec::new();
        FormatInterne.ecrire(&e, &mut sortie).unwrap();
        assert_eq!(sortie, b"{\"t\":0.5,\"ev\":\"inertie\",\"sol\":\"s2\"}\n");
        let ligne = std::str::from_utf8(&sortie).unwrap().trim_end();
        assert_eq!(FormatInterne.lire(ligne).unwrap(), e);
        assert!(FormatInterne.lire("{").is_err());
    }
}
