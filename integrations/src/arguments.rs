//! Analyse des arguments avec Clap.
//!
//! Point d'integration : `chem::cli::arguments::AnalyseurArguments`.
//!
//! Les commandes Clap sont construites a partir des tables de declaration
//! de la bibliotheque (`chem::cli::RUN`, `chem::cli::CHECK`...) : la ligne
//! de commande n'est decrite qu'a un endroit, qui sert aussi a l'aide
//! (`chem <commande> --help` reste rendue par la bibliotheque, en
//! francais). L'aiguillage vers la sous-commande, `--help` et `--version`
//! au niveau global restent dans `chem::cli`.
//!
//! Les deux analyseurs acceptent la meme syntaxe et produisent les memes
//! `Arguments` :
//!
//! - `--option valeur` et `--option=valeur` ;
//! - valeur commencant par un tiret (`--trace-jsonl -`, `--seed -1`) ;
//! - option repetee : la derniere occurrence l'emporte ;
//! - `-h` ou `--help` a n'importe quelle position ;
//! - arguments positionnels comptes par la bibliotheque
//!   (`chem::cli::arguments::verifier_positionnels`), avec les memes
//!   messages.
//!
//! Seul ajout de Clap : `--` termine les options (ce qui suit est
//! positionnel). Les erreurs courantes (option inconnue, valeur manquante,
//! valeur donnee a un drapeau) sont traduites avec les messages de la
//! bibliotheque ; les autres gardent le message de Clap.

use chem::cli::arguments::{
    AnalyseurArguments, Arguments, DefinitionCommande, verifier_positionnels,
};
use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{Arg, ArgAction, Command, value_parser};

/// Identifiant Clap du drapeau d'aide.
const AIDE: &str = "aide";
/// Identifiant Clap des arguments positionnels.
const POSITIONNELS: &str = "positionnels";

/// Analyseur Clap.
#[derive(Debug, Default, Clone, Copy)]
pub struct AnalyseurClap;

impl AnalyseurArguments for AnalyseurClap {
    fn nom(&self) -> &'static str {
        "clap"
    }

    fn analyser(
        &self,
        definition: &DefinitionCommande,
        arguments: &[String],
    ) -> Result<Arguments, String> {
        let correspondances = commande(definition)
            .try_get_matches_from(arguments)
            .map_err(|erreur| traduire(&erreur, definition))?;
        let mut resultat = Arguments::default();
        for option in definition.options {
            if option.valeur.is_some() {
                if let Some(valeur) = correspondances.get_one::<String>(option.nom) {
                    resultat.definir(option.nom, valeur.clone());
                }
            } else if correspondances.get_flag(option.nom) {
                resultat.lever(option.nom);
            }
        }
        if let Some(valeurs) = correspondances.get_many::<String>(POSITIONNELS) {
            resultat.positionnels = valeurs.cloned().collect();
        }
        resultat.aide = correspondances.get_flag(AIDE);
        verifier_positionnels(definition, &resultat)?;
        Ok(resultat)
    }
}

/// Commande Clap equivalente a une declaration de la bibliotheque.
pub fn commande(definition: &DefinitionCommande) -> Command {
    let mut commande = Command::new(definition.nom)
        .about(definition.resume)
        .no_binary_name(true)
        .disable_help_flag(true)
        .disable_version_flag(true)
        .args_override_self(true)
        .arg(
            Arg::new(AIDE)
                .short('h')
                .long("help")
                .help("affiche cette aide")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(POSITIONNELS)
                .value_name("ARGUMENT")
                .action(ArgAction::Append)
                .value_parser(value_parser!(String)),
        );
    for option in definition.options {
        let argument = Arg::new(option.nom).long(option.nom).help(option.aide);
        commande = commande.arg(match option.valeur {
            None => argument.action(ArgAction::SetTrue),
            Some(metavariable) => argument
                .value_name(metavariable)
                .action(ArgAction::Set)
                .allow_hyphen_values(true)
                .value_parser(value_parser!(String)),
        });
    }
    commande
}

/// Message d'erreur dans les termes de l'analyseur de la bibliotheque.
fn traduire(erreur: &clap::Error, definition: &DefinitionCommande) -> String {
    let contexte = |cle: ContextKind| match erreur.get(cle) {
        Some(ContextValue::String(texte)) => Some(texte.clone()),
        Some(ContextValue::Strings(textes)) => textes.first().cloned(),
        _ => None,
    };
    // Clap designe une option par son usage ('--seed <N>') : seul le nom compte.
    let option = contexte(ContextKind::InvalidArg)
        .and_then(|usage| usage.split_whitespace().next().map(str::to_string));
    match (erreur.kind(), option) {
        (ErrorKind::UnknownArgument, Some(option)) => {
            let mut message = if option.starts_with("--") {
                format!("option inconnue '{option}' pour 'chem {}'", definition.nom)
            } else {
                format!("option inconnue '{option}'")
            };
            if let Some(proche) = contexte(ContextKind::SuggestedArg) {
                message.push_str(&format!(" (vouliez-vous dire '{proche}' ?)"));
            }
            message
        }
        (ErrorKind::TooManyValues, Some(option)) => {
            format!("l'option '{option}' n'attend pas de valeur")
        }
        (ErrorKind::InvalidValue, Some(option)) => {
            match definition
                .options
                .iter()
                .find(|o| option.strip_prefix("--") == Some(o.nom))
                .and_then(|o| o.valeur)
            {
                Some(metavariable) => {
                    format!("l'option '{option}' attend une valeur <{metavariable}>")
                }
                None => premiere_ligne(erreur),
            }
        }
        _ => premiere_ligne(erreur),
    }
}

/// Premiere ligne du message de Clap, sans le prefixe `error: `.
fn premiere_ligne(erreur: &clap::Error) -> String {
    let texte = erreur.to_string();
    let ligne = texte.lines().next().unwrap_or_default();
    ligne.strip_prefix("error: ").unwrap_or(ligne).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem::cli::COMMANDES;

    #[test]
    fn commandes_clap_valides() {
        // `debug_assert` de Clap : verifie la coherence de chaque commande.
        for definition in COMMANDES {
            commande(definition).debug_assert();
        }
    }
}
