//! Rendu des diagnostics avec Miette.
//!
//! Point d'integration : `chem::diagnostic::RenduDiagnostic`.
//!
//! Chaque diagnostic de la bibliotheque est presente a Miette comme un
//! `miette::Diagnostic` : code stable (`E0201`), gravite (erreur ou
//! avertissement), message, etiquettes (la premiere est l'etiquette
//! principale) et aide. Le rendu graphique de Miette remplace l'extrait
//! souligne de la bibliotheque ; il affiche les memes informations.
//!
//! Couleurs : seulement si la sortie d'erreur est un terminal et que la
//! variable d'environnement `NO_COLOR` est absente ou vide. Si Miette ne
//! peut pas produire le rendu, le rendu texte de la bibliotheque est
//! utilise : un diagnostic n'est jamais perdu.

use chem::diagnostic::{Diagnostic, Gravite, RenduDiagnostic, rendre};
use chem::source::Source;
use miette::{
    GraphicalReportHandler, GraphicalTheme, LabeledSpan, NamedSource, Severity, SourceCode,
    SourceSpan,
};
use std::fmt;
use std::io::IsTerminal;

/// Rendu Miette.
#[derive(Debug, Clone, Copy)]
pub struct RenduMiette {
    couleurs: bool,
}

impl RenduMiette {
    /// Couleurs selon la sortie d'erreur (terminal) et `NO_COLOR`.
    pub fn automatique() -> Self {
        let couleurs = std::io::stderr().is_terminal()
            && std::env::var_os("NO_COLOR").is_none_or(|valeur| valeur.is_empty());
        Self { couleurs }
    }

    /// Sans couleurs (sortie redirigee, tests).
    pub fn sans_couleurs() -> Self {
        Self { couleurs: false }
    }
}

impl Default for RenduMiette {
    fn default() -> Self {
        Self::automatique()
    }
}

impl RenduDiagnostic for RenduMiette {
    fn nom(&self) -> &'static str {
        "miette"
    }

    fn rendre(&self, diagnostic: &Diagnostic, source: &Source) -> String {
        let theme = if self.couleurs {
            GraphicalTheme::unicode()
        } else {
            GraphicalTheme::unicode_nocolor()
        };
        let rapport = Rapport::new(diagnostic, source);
        let mut texte = String::new();
        match GraphicalReportHandler::new_themed(theme).render_report(&mut texte, &rapport) {
            Ok(()) => {
                if !texte.ends_with('\n') {
                    texte.push('\n');
                }
                texte
            }
            Err(_) => rendre(diagnostic, source),
        }
    }
}

/// Diagnostic de la bibliotheque vu par Miette.
struct Rapport<'a> {
    diagnostic: &'a Diagnostic,
    source: NamedSource<String>,
    longueur: usize,
}

impl<'a> Rapport<'a> {
    fn new(diagnostic: &'a Diagnostic, source: &Source) -> Self {
        Self {
            diagnostic,
            source: NamedSource::new(source.nom(), source.texte().to_string()),
            longueur: source.texte().len(),
        }
    }

    /// Zone de Miette (debut, longueur), bornee au texte.
    fn zone(&self, debut: usize, fin: usize) -> SourceSpan {
        let debut = debut.min(self.longueur);
        let fin = fin.clamp(debut, self.longueur);
        SourceSpan::from((debut, fin - debut))
    }
}

impl fmt::Debug for Rapport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rapport")
            .field("diagnostic", self.diagnostic)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for Rapport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.diagnostic.message)
    }
}

impl std::error::Error for Rapport<'_> {}

impl miette::Diagnostic for Rapport<'_> {
    fn code<'b>(&'b self) -> Option<Box<dyn fmt::Display + 'b>> {
        Some(Box::new(self.diagnostic.code))
    }

    fn severity(&self) -> Option<Severity> {
        Some(match self.diagnostic.gravite {
            Gravite::Erreur => Severity::Error,
            Gravite::Avertissement => Severity::Warning,
        })
    }

    fn help<'b>(&'b self) -> Option<Box<dyn fmt::Display + 'b>> {
        self.diagnostic
            .aide
            .as_ref()
            .map(|aide| Box::new(aide) as Box<dyn fmt::Display + 'b>)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        Some(&self.source)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        if self.diagnostic.etiquettes.is_empty() {
            return None;
        }
        Some(Box::new(self.diagnostic.etiquettes.iter().enumerate().map(
            |(rang, etiquette)| {
                let message = (!etiquette.message.is_empty()).then(|| etiquette.message.clone());
                let zone = self.zone(etiquette.span.debut, etiquette.span.fin);
                if rang == 0 {
                    LabeledSpan::new_primary_with_span(message, zone)
                } else {
                    LabeledSpan::new_with_span(message, zone)
                }
            },
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem::source::Span;

    /// Caracteres ASCII visibles seulement (lignes repliees, cadre graphique).
    fn compact(texte: &str) -> String {
        texte.chars().filter(char::is_ascii_graphic).collect()
    }

    #[test]
    fn rendu_complet_sans_couleurs() {
        let source = Source::new("b1.chem", "let max = replace x, y by x\n{ 1 | max }\n");
        let diagnostic = Diagnostic::erreur("E0201", "';' attendu a la fin de la definition")
            .etiquette(Span::new(27, 27), "';' attendu ici")
            .aide("chaque definition se termine par ';'");
        let texte = RenduMiette::sans_couleurs().rendre(&diagnostic, &source);
        let compacte = compact(&texte);
        for attendu in [
            "E0201",
            "';' attendu a la fin de la definition",
            "';' attendu ici",
            "chaque definition se termine par ';'",
            "b1.chem",
        ] {
            assert!(
                compacte.contains(&compact(attendu)),
                "{attendu} absent de :\n{texte}"
            );
        }
        assert!(!texte.contains('\u{1b}'), "pas de couleurs :\n{texte}");
    }

    #[test]
    fn zone_hors_du_texte_bornee() {
        let source = Source::new("vide.chem", "");
        let diagnostic = Diagnostic::avertissement("W0301", "programme vide")
            .etiquette(Span::new(10, 20), "ici");
        let texte = RenduMiette::sans_couleurs().rendre(&diagnostic, &source);
        assert!(texte.contains("W0301"), "{texte}");
    }
}
