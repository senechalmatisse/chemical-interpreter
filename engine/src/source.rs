//! Texte source et positions lisibles (fichier, ligne, colonne).

use std::fmt;
use std::io;
use std::path::Path;

/// Intervalle d'octets `[debut, fin)` dans un texte source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// Premier octet couvert (inclus).
    pub debut: usize,
    /// Fin de l'intervalle (exclue).
    pub fin: usize,
}

impl Span {
    /// Construit un intervalle.
    pub const fn new(debut: usize, fin: usize) -> Self {
        Self { debut, fin }
    }

    /// Plus petit intervalle couvrant `self` et `autre`.
    pub fn fusion(self, autre: Span) -> Span {
        Span::new(self.debut.min(autre.debut), self.fin.max(autre.fin))
    }

    /// Nombre d'octets couverts.
    pub fn longueur(&self) -> usize {
        self.fin.saturating_sub(self.debut)
    }
}

/// Position lisible par un humain, numerotee a partir de 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Numero de ligne (1 = premiere ligne).
    pub ligne: usize,
    /// Numero de colonne en caracteres (1 = premier caractere).
    pub colonne: usize,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.ligne, self.colonne)
    }
}

/// Texte source nomme, avec l'index des debuts de ligne.
#[derive(Debug, Clone)]
pub struct Source {
    nom: String,
    texte: String,
    debuts_lignes: Vec<usize>,
}

impl Source {
    /// Construit une source a partir d'un nom (fichier) et d'un texte.
    pub fn new(nom: impl Into<String>, texte: impl Into<String>) -> Self {
        let texte = texte.into();
        let mut debuts_lignes = vec![0];
        debuts_lignes.extend(texte.match_indices('\n').map(|(i, _)| i + 1));
        Self {
            nom: nom.into(),
            texte,
            debuts_lignes,
        }
    }

    /// Lit un fichier source (UTF-8).
    pub fn depuis_fichier(chemin: &Path) -> io::Result<Self> {
        let texte = std::fs::read_to_string(chemin)?;
        Ok(Self::new(chemin.display().to_string(), texte))
    }

    /// Nom de la source (chemin du fichier en general).
    pub fn nom(&self) -> &str {
        &self.nom
    }

    /// Texte complet.
    pub fn texte(&self) -> &str {
        &self.texte
    }

    /// Nombre de lignes du texte.
    pub fn nombre_lignes(&self) -> usize {
        self.debuts_lignes.len()
    }

    /// Convertit un decalage en octets en position (ligne, colonne).
    pub fn position(&self, decalage: usize) -> Position {
        let mut decalage = decalage.min(self.texte.len());
        while !self.texte.is_char_boundary(decalage) {
            decalage -= 1;
        }
        let ligne = match self.debuts_lignes.binary_search(&decalage) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let debut = self.debuts_lignes[ligne];
        let colonne = self.texte[debut..decalage].chars().count() + 1;
        Position {
            ligne: ligne + 1,
            colonne,
        }
    }

    /// Texte d'une ligne (numerotee a partir de 1), sans fin de ligne.
    pub fn ligne(&self, numero: usize) -> &str {
        let index = numero.clamp(1, self.debuts_lignes.len()) - 1;
        let debut = self.debuts_lignes[index];
        let fin = self
            .debuts_lignes
            .get(index + 1)
            .map_or(self.texte.len(), |&f| f);
        self.texte[debut..fin].trim_end_matches(['\n', '\r'])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_et_lignes() {
        let s = Source::new("t.chem", "ab\ncd\n\nef");
        assert_eq!(
            s.position(0),
            Position {
                ligne: 1,
                colonne: 1
            }
        );
        assert_eq!(
            s.position(3),
            Position {
                ligne: 2,
                colonne: 1
            }
        );
        assert_eq!(
            s.position(4),
            Position {
                ligne: 2,
                colonne: 2
            }
        );
        assert_eq!(
            s.position(6),
            Position {
                ligne: 3,
                colonne: 1
            }
        );
        assert_eq!(
            s.position(100),
            Position {
                ligne: 4,
                colonne: 3
            }
        );
        assert_eq!(s.ligne(2), "cd");
        assert_eq!(s.ligne(3), "");
        assert_eq!(s.nombre_lignes(), 4);
    }

    #[test]
    fn colonnes_en_caracteres() {
        let s = Source::new("t", "# e\u{301}te\nx");
        assert_eq!(s.position(s.texte().len() - 1).ligne, 2);
        // decalage au milieu d'un caractere multi-octets : pas de panique
        let p = s.position(4);
        assert_eq!(p.ligne, 1);
    }

    #[test]
    fn fusion_de_spans() {
        assert_eq!(Span::new(3, 5).fusion(Span::new(1, 4)), Span::new(1, 5));
        assert_eq!(Span::new(3, 5).longueur(), 2);
    }
}
