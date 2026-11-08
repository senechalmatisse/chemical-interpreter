//! Analyse declarative des arguments de la ligne de commande.
//!
//! Chaque commande declare ses options dans une table
//! ([`DefinitionCommande`]) qui sert a la fois a l'analyse et a l'aide.
//! L'analyse passe par l'interface [`AnalyseurArguments`] :
//! [`AnalyseurInterne`] est celui de la bibliotheque ; le paquet
//! `integrations` en fournit un avec Clap, construit a partir des memes
//! tables. Les deux produisent les memes [`Arguments`].

use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Option declaree par une commande.
#[derive(Debug, Clone, Copy)]
pub struct DefinitionOption {
    /// Nom long (sans `--`).
    pub nom: &'static str,
    /// Nom de la valeur attendue (`None` : simple drapeau).
    pub valeur: Option<&'static str>,
    /// Aide.
    pub aide: &'static str,
}

/// Declaration d'une commande.
#[derive(Debug, Clone, Copy)]
pub struct DefinitionCommande {
    /// Nom de la sous-commande.
    pub nom: &'static str,
    /// Arguments positionnels (noms).
    pub positionnels: &'static [&'static str],
    /// Nombre de positionnels obligatoires.
    pub obligatoires: usize,
    /// Options.
    pub options: &'static [DefinitionOption],
    /// Description.
    pub resume: &'static str,
}

/// Arguments analyses.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Arguments {
    valeurs: HashMap<&'static str, String>,
    drapeaux: HashSet<&'static str>,
    /// Arguments positionnels.
    pub positionnels: Vec<String>,
    /// `--help` demande.
    pub aide: bool,
}

impl Arguments {
    /// Valeur brute d'une option.
    pub fn valeur(&self, nom: &str) -> Option<&str> {
        self.valeurs.get(nom).map(String::as_str)
    }

    /// Drapeau present.
    pub fn drapeau(&self, nom: &str) -> bool {
        self.drapeaux.contains(nom)
    }

    /// Enregistre la valeur d'une option (la derniere l'emporte).
    pub fn definir(&mut self, option: &'static str, valeur: impl Into<String>) {
        self.valeurs.insert(option, valeur.into());
    }

    /// Leve un drapeau.
    pub fn lever(&mut self, drapeau: &'static str) {
        self.drapeaux.insert(drapeau);
    }

    /// Entier non signe facultatif.
    pub fn entier(&self, nom: &str) -> Result<Option<u64>, String> {
        self.valeur(nom)
            .map(|v| {
                v.replace('_', "")
                    .parse::<u64>()
                    .map_err(|_| format!("--{nom} : entier positif attendu, trouve '{v}'"))
            })
            .transpose()
    }

    /// Duree facultative (`5`, `1.5s`, `500ms`, `2m`).
    pub fn duree(&self, nom: &str) -> Result<Option<Duration>, String> {
        self.valeur(nom)
            .map(|v| {
                analyser_duree(v).ok_or_else(|| {
                    format!("--{nom} : duree invalide '{v}' (exemples : 5, 1.5s, 500ms, 2m)")
                })
            })
            .transpose()
    }
}

/// Analyse une duree : nombre de secondes, ou suffixe `ms`, `s`, `m`.
pub fn analyser_duree(texte: &str) -> Option<Duration> {
    let texte = texte.trim();
    let (nombre, facteur) = if let Some(n) = texte.strip_suffix("ms") {
        (n, 0.001)
    } else if let Some(n) = texte.strip_suffix('s') {
        (n, 1.0)
    } else if let Some(n) = texte.strip_suffix('m') {
        (n, 60.0)
    } else {
        (texte, 1.0)
    };
    let valeur: f64 = nombre.trim().parse().ok()?;
    (valeur.is_finite() && valeur >= 0.0).then(|| Duration::from_secs_f64(valeur * facteur))
}

/// Analyse des arguments d'une sous-commande.
pub trait AnalyseurArguments: Send + Sync {
    /// Nom de l'implementation (affiche par `chem --version`).
    fn nom(&self) -> &'static str;

    /// Analyse les arguments qui suivent le nom de la sous-commande.
    fn analyser(
        &self,
        definition: &DefinitionCommande,
        arguments: &[String],
    ) -> Result<Arguments, String>;
}

/// Analyseur de la bibliotheque ([`analyser`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct AnalyseurInterne;

impl AnalyseurArguments for AnalyseurInterne {
    fn nom(&self) -> &'static str {
        "interne"
    }

    fn analyser(
        &self,
        definition: &DefinitionCommande,
        arguments: &[String],
    ) -> Result<Arguments, String> {
        analyser(definition, arguments)
    }
}

/// Verifie le nombre d'arguments positionnels (sauf si l'aide est demandee).
pub fn verifier_positionnels(
    definition: &DefinitionCommande,
    arguments: &Arguments,
) -> Result<(), String> {
    if arguments.aide {
        return Ok(());
    }
    if arguments.positionnels.len() < definition.obligatoires {
        return Err(format!(
            "argument manquant : <{}> (voir 'chem {} --help')",
            definition.positionnels[arguments.positionnels.len()],
            definition.nom
        ));
    }
    if arguments.positionnels.len() > definition.positionnels.len() {
        return Err(format!(
            "argument inattendu '{}'",
            arguments.positionnels[definition.positionnels.len()]
        ));
    }
    Ok(())
}

/// Analyse les arguments d'une commande.
pub fn analyser(
    definition: &DefinitionCommande,
    arguments: &[String],
) -> Result<Arguments, String> {
    let mut resultat = Arguments::default();
    let mut i = 0;
    while i < arguments.len() {
        let argument = &arguments[i];
        i += 1;
        if argument == "-h" || argument == "--help" {
            resultat.aide = true;
            continue;
        }
        let Some(long) = argument.strip_prefix("--") else {
            if argument.starts_with('-') && argument.len() > 1 {
                return Err(format!("option inconnue '{argument}'"));
            }
            resultat.positionnels.push(argument.clone());
            continue;
        };
        let (nom, valeur_jointe) = match long.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (long, None),
        };
        let Some(option) = definition.options.iter().find(|o| o.nom == nom) else {
            let mut message = format!("option inconnue '--{nom}' pour 'chem {}'", definition.nom);
            if let Some(proche) =
                crate::semantique::suggestion(nom, definition.options.iter().map(|o| o.nom))
            {
                message.push_str(&format!(" (vouliez-vous dire '--{proche}' ?)"));
            }
            return Err(message);
        };
        match option.valeur {
            None => {
                if valeur_jointe.is_some() {
                    return Err(format!("l'option '--{nom}' n'attend pas de valeur"));
                }
                resultat.drapeaux.insert(option.nom);
            }
            Some(metavariable) => {
                let valeur = match valeur_jointe {
                    Some(v) => v,
                    None if i < arguments.len() => {
                        i += 1;
                        arguments[i - 1].clone()
                    }
                    None => {
                        return Err(format!(
                            "l'option '--{nom}' attend une valeur <{metavariable}>"
                        ));
                    }
                };
                resultat.valeurs.insert(option.nom, valeur);
            }
        }
    }
    verifier_positionnels(definition, &resultat)?;
    Ok(resultat)
}

/// Aide d'une commande, generee a partir de sa declaration.
pub fn aide(definition: &DefinitionCommande) -> String {
    let mut texte = format!("{}\n\nUsage : chem {}", definition.resume, definition.nom);
    for (i, p) in definition.positionnels.iter().enumerate() {
        if i < definition.obligatoires {
            texte.push_str(&format!(" <{p}>"));
        } else {
            texte.push_str(&format!(" [{p}]"));
        }
    }
    texte.push_str(" [options]\n\nOptions :\n");
    let largeur = definition
        .options
        .iter()
        .map(|o| o.nom.len() + o.valeur.map_or(0, |v| v.len() + 3))
        .max()
        .unwrap_or(0)
        .max(4);
    for o in definition.options {
        let gauche = match o.valeur {
            Some(v) => format!("{} <{v}>", o.nom),
            None => o.nom.to_string(),
        };
        texte.push_str(&format!("  --{gauche:<largeur$}  {}\n", o.aide));
    }
    texte.push_str(&format!(
        "  --{:<largeur$}  {}\n",
        "help", "affiche cette aide"
    ));
    texte
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEF: DefinitionCommande = DefinitionCommande {
        nom: "run",
        positionnels: &["fichier"],
        obligatoires: 1,
        options: &[
            DefinitionOption {
                nom: "workers",
                valeur: Some("N"),
                aide: "",
            },
            DefinitionOption {
                nom: "trace",
                valeur: None,
                aide: "",
            },
            DefinitionOption {
                nom: "delai-max",
                valeur: Some("DUREE"),
                aide: "",
            },
        ],
        resume: "Execute",
    };

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn options_et_positionnels() {
        let a = analyser(
            &DEF,
            &args(&["p.chem", "--workers", "4", "--trace", "--delai-max=500ms"]),
        )
        .unwrap();
        assert_eq!(a.positionnels, vec!["p.chem"]);
        assert_eq!(a.entier("workers").unwrap(), Some(4));
        assert!(a.drapeau("trace"));
        assert_eq!(
            a.duree("delai-max").unwrap(),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn erreurs() {
        assert!(
            analyser(&DEF, &args(&[]))
                .unwrap_err()
                .contains("argument manquant")
        );
        assert!(
            analyser(&DEF, &args(&["a", "b"]))
                .unwrap_err()
                .contains("inattendu")
        );
        assert!(
            analyser(&DEF, &args(&["a", "--worker", "2"]))
                .unwrap_err()
                .contains("--workers")
        );
        assert!(
            analyser(&DEF, &args(&["a", "--workers"]))
                .unwrap_err()
                .contains("attend une valeur")
        );
        assert!(analyser(&DEF, &args(&["a", "--trace=1"])).is_err());
        assert!(analyser(&DEF, &args(&["a", "-x"])).is_err());
        let a = analyser(&DEF, &args(&["a", "--workers", "deux"])).unwrap();
        assert!(a.entier("workers").is_err());
        assert!(analyser(&DEF, &args(&["--help"])).unwrap().aide);
    }

    #[test]
    fn durees() {
        assert_eq!(analyser_duree("5"), Some(Duration::from_secs(5)));
        assert_eq!(analyser_duree("1.5s"), Some(Duration::from_millis(1500)));
        assert_eq!(analyser_duree("2m"), Some(Duration::from_secs(120)));
        assert_eq!(analyser_duree("-1"), None);
        assert_eq!(analyser_duree("abc"), None);
    }

    #[test]
    fn aide_generee() {
        let texte = aide(&DEF);
        assert!(texte.contains("Usage : chem run <fichier> [options]"));
        assert!(texte.contains("--workers <N>"));
    }
}
