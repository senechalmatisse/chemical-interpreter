//! Configuration d'une execution.

use crate::alea::Alea;
use std::fmt;
use std::time::Duration;

/// Ordre de selection des reactions.
///
/// C'est une *valeur* de configuration, pas une famille d'algorithmes :
/// l'unique algorithme de recherche parcourt les elements dans l'ordre du
/// lot ; en mode aleatoire, cet ordre (et celui des regles) est melange par
/// un generateur deterministe initialise par la graine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Strategie {
    /// Ordre pseudo-aleatoire rejouable (graine).
    #[default]
    Aleatoire,
    /// Ordre du source, deterministe.
    Ordonnee,
}

impl Strategie {
    /// Nom dans la ligne de commande et la trace.
    pub fn nom(&self) -> &'static str {
        match self {
            Strategie::Aleatoire => "aleatoire",
            Strategie::Ordonnee => "ordonnee",
        }
    }

    /// Analyse un nom.
    pub fn depuis_nom(nom: &str) -> Option<Self> {
        match nom {
            "aleatoire" => Some(Strategie::Aleatoire),
            "ordonnee" => Some(Strategie::Ordonnee),
            _ => None,
        }
    }
}

impl fmt::Display for Strategie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.nom())
    }
}

/// Limites d'execution (garde-fous).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limites {
    /// Nombre maximal de reactions.
    pub max_etapes: Option<u64>,
    /// Duree maximale.
    pub delai_max: Option<Duration>,
    /// Nombre maximal de molecules (approximation de la memoire).
    pub max_molecules: Option<u64>,
}

/// Limite de molecules par defaut (environ quelques centaines de Mo).
pub const MAX_MOLECULES_DEFAUT: u64 = 10_000_000;

impl Default for Limites {
    fn default() -> Self {
        Self {
            max_etapes: None,
            delai_max: None,
            max_molecules: Some(MAX_MOLECULES_DEFAUT),
        }
    }
}

/// Parametres d'une execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    workers: usize,
    strategie: Strategie,
    graine: u64,
    limites: Limites,
    indexation: bool,
    partition: bool,
}

impl Default for Configuration {
    fn default() -> Self {
        Self::constructeur().construire()
    }
}

impl Configuration {
    /// Commence une configuration.
    pub fn constructeur() -> ConstructeurConfiguration {
        ConstructeurConfiguration::default()
    }

    /// Nombre de workers (>= 1).
    pub fn workers(&self) -> usize {
        self.workers
    }

    /// Strategie de selection.
    pub fn strategie(&self) -> Strategie {
        self.strategie
    }

    /// Graine effective.
    pub fn graine(&self) -> u64 {
        self.graine
    }

    /// Limites.
    pub fn limites(&self) -> &Limites {
        &self.limites
    }

    /// Recherche des partenaires par valeur pour les egalites (defaut : oui).
    pub fn indexation(&self) -> bool {
        self.indexation
    }

    /// Repartition des lots guidee par les jointures : par cle, et par
    /// segments pour une chaine (defaut : oui).
    pub fn partition(&self) -> bool {
        self.partition
    }
}

/// Monteur de [`Configuration`] : valeurs par defaut sures, validation au
/// moment de `construire`.
#[derive(Debug, Clone, Default)]
pub struct ConstructeurConfiguration {
    workers: Option<usize>,
    strategie: Strategie,
    graine: Option<u64>,
    limites: Limites,
    sans_index: bool,
    sans_partition: bool,
}

impl ConstructeurConfiguration {
    /// Nombre de workers (0 est ramene a 1).
    pub fn workers(mut self, n: usize) -> Self {
        self.workers = Some(n.max(1));
        self
    }

    /// Strategie.
    pub fn strategie(mut self, s: Strategie) -> Self {
        self.strategie = s;
        self
    }

    /// Graine (sinon tiree de l'horloge en mode aleatoire, 0 en mode ordonne).
    pub fn graine(mut self, g: u64) -> Self {
        self.graine = Some(g);
        self
    }

    /// Graine facultative.
    pub fn graine_optionnelle(mut self, g: Option<u64>) -> Self {
        self.graine = g;
        self
    }

    /// Nombre maximal de reactions.
    pub fn max_etapes(mut self, n: Option<u64>) -> Self {
        self.limites.max_etapes = n;
        self
    }

    /// Duree maximale.
    pub fn delai_max(mut self, d: Option<Duration>) -> Self {
        self.limites.delai_max = d;
        self
    }

    /// Nombre maximal de molecules (`None` : pas de limite).
    pub fn max_molecules(mut self, n: Option<u64>) -> Self {
        self.limites.max_molecules = n;
        self
    }

    /// Indexation des molecules par valeur pour les conditions d'egalite
    /// (desactivable pour comparer ou mesurer).
    pub fn indexation(mut self, active: bool) -> Self {
        self.sans_index = !active;
        self
    }

    /// Repartition des lots guidee par les jointures (desactivable pour
    /// comparer ou mesurer).
    pub fn partition(mut self, active: bool) -> Self {
        self.sans_partition = !active;
        self
    }

    /// Construit la configuration.
    pub fn construire(self) -> Configuration {
        let graine = self.graine.unwrap_or(match self.strategie {
            Strategie::Aleatoire => Alea::graine_horloge(),
            Strategie::Ordonnee => 0,
        });
        Configuration {
            workers: self.workers.unwrap_or(1),
            strategie: self.strategie,
            graine,
            limites: self.limites,
            indexation: !self.sans_index,
            partition: !self.sans_partition,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valeurs_par_defaut_et_monteur() {
        let c = Configuration::constructeur()
            .strategie(Strategie::Ordonnee)
            .workers(0)
            .max_etapes(Some(10))
            .construire();
        assert_eq!(c.workers(), 1);
        assert_eq!(c.graine(), 0);
        assert_eq!(c.limites().max_etapes, Some(10));
        assert_eq!(c.limites().max_molecules, Some(MAX_MOLECULES_DEFAUT));
        assert!(c.indexation() && c.partition());
        assert!(
            !Configuration::constructeur()
                .partition(false)
                .construire()
                .partition()
        );
        assert!(
            !Configuration::constructeur()
                .indexation(false)
                .construire()
                .indexation()
        );
        assert_eq!(Strategie::depuis_nom("ordonnee"), Some(Strategie::Ordonnee));
        assert_eq!(Strategie::depuis_nom("x"), None);
    }
}
