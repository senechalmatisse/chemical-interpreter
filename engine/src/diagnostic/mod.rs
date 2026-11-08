//! Diagnostics localises (erreurs et avertissements).
//!
//! Un [`Diagnostic`] porte un code stable (`E0201`, `W0302`...), un message,
//! des etiquettes pointant dans le source et une aide facultative. Le rendu
//! textuel (extrait de source souligne) est assure par [`rendre`].
//!
//! Point d'integration : ce module joue le role de Miette. Pour passer a
//! Miette, il suffit d'implementer `miette::Diagnostic` pour [`Diagnostic`]
//! (code, labels, help) et de remplacer [`rendre`] par le rapport graphique.

mod rendu;

pub use rendu::rendre;

use crate::source::Span;
use std::fmt;

/// Gravite d'un diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gravite {
    /// Empeche l'execution.
    Erreur,
    /// Signale une construction suspecte sans bloquer.
    Avertissement,
}

/// Zone du source commentee par un diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Etiquette {
    /// Zone concernee.
    pub span: Span,
    /// Commentaire affiche sous la zone.
    pub message: String,
}

/// Erreur ou avertissement localise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Gravite.
    pub gravite: Gravite,
    /// Code stable (voir docs/langage.md, annexe des codes).
    pub code: &'static str,
    /// Message principal.
    pub message: String,
    /// Etiquettes ; la premiere est l'etiquette principale.
    pub etiquettes: Vec<Etiquette>,
    /// Conseil facultatif.
    pub aide: Option<String>,
}

impl Diagnostic {
    /// Cree une erreur.
    pub fn erreur(code: &'static str, message: impl Into<String>) -> Self {
        Self::nouveau(Gravite::Erreur, code, message)
    }

    /// Cree un avertissement.
    pub fn avertissement(code: &'static str, message: impl Into<String>) -> Self {
        Self::nouveau(Gravite::Avertissement, code, message)
    }

    fn nouveau(gravite: Gravite, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            gravite,
            code,
            message: message.into(),
            etiquettes: Vec::new(),
            aide: None,
        }
    }

    /// Ajoute une etiquette (la premiere ajoutee est la principale).
    pub fn etiquette(mut self, span: Span, message: impl Into<String>) -> Self {
        self.etiquettes.push(Etiquette {
            span,
            message: message.into(),
        });
        self
    }

    /// Ajoute une aide.
    pub fn aide(mut self, aide: impl Into<String>) -> Self {
        self.aide = Some(aide.into());
        self
    }

    /// Zone principale du diagnostic, s'il en a une.
    pub fn span(&self) -> Option<Span> {
        self.etiquettes.first().map(|e| e.span)
    }

    /// Vrai pour une erreur.
    pub fn est_erreur(&self) -> bool {
        self.gravite == Gravite::Erreur
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let genre = match self.gravite {
            Gravite::Erreur => "erreur",
            Gravite::Avertissement => "avertissement",
        };
        write!(f, "{genre}[{}]: {}", self.code, self.message)
    }
}

impl std::error::Error for Diagnostic {}

/// Collection ordonnee de diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    liste: Vec<Diagnostic>,
}

impl Diagnostics {
    /// Collection vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute un diagnostic.
    pub fn ajouter(&mut self, diagnostic: Diagnostic) {
        self.liste.push(diagnostic);
    }

    /// Ajoute tous les diagnostics d'une autre collection.
    pub fn etendre(&mut self, autres: Diagnostics) {
        self.liste.extend(autres.liste);
    }

    /// Vrai si au moins une erreur est presente.
    pub fn a_des_erreurs(&self) -> bool {
        self.liste.iter().any(Diagnostic::est_erreur)
    }

    /// Erreurs seulement.
    pub fn erreurs(&self) -> impl Iterator<Item = &Diagnostic> {
        self.liste.iter().filter(|d| d.est_erreur())
    }

    /// Avertissements seulement.
    pub fn avertissements(&self) -> impl Iterator<Item = &Diagnostic> {
        self.liste.iter().filter(|d| !d.est_erreur())
    }

    /// Tous les diagnostics, dans l'ordre d'ajout.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.liste.iter()
    }

    /// Nombre de diagnostics.
    pub fn len(&self) -> usize {
        self.liste.len()
    }

    /// Vrai si la collection est vide.
    pub fn is_empty(&self) -> bool {
        self.liste.is_empty()
    }

    /// Diagnostics tries par position (ceux sans position en dernier).
    pub fn tries(&self) -> Vec<&Diagnostic> {
        let mut v: Vec<&Diagnostic> = self.liste.iter().collect();
        v.sort_by_key(|d| d.span().map_or(usize::MAX, |s| s.debut));
        v
    }
}

impl From<Diagnostic> for Diagnostics {
    fn from(d: Diagnostic) -> Self {
        Self { liste: vec![d] }
    }
}

impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.liste.into_iter()
    }
}

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for d in &self.liste {
            writeln!(f, "{d}")?;
        }
        Ok(())
    }
}
