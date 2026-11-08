//! Outils partages par les tests d'integration.
#![allow(dead_code)]

use chem::Interpreteur;
use chem::alea::Alea;
use chem::ordonnanceur::{Configuration, Execution, Strategie};
use chem::rejeu;
use chem::source::Source;
use chem::trace::{Diffuseur, Enregistreur, Evenement};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

/// Racine du depot (contient `exemples/`).
pub fn depot() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Programme d'exemple : chemin, texte et resultat attendu (`# attendu : ...`).
pub struct Exemple {
    pub chemin: PathBuf,
    pub texte: String,
    pub attendu: Option<String>,
}

/// Programmes de `exemples/` (hors sous-repertoires), tries par nom.
pub fn exemples() -> Vec<Exemple> {
    let mut chemins: Vec<PathBuf> = std::fs::read_dir(depot().join("exemples"))
        .expect("repertoire exemples")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "chem"))
        .collect();
    chemins.sort();
    chemins
        .into_iter()
        .map(|chemin| {
            let texte = std::fs::read_to_string(&chemin).expect("lecture");
            let attendu = texte
                .lines()
                .find_map(|l| l.strip_prefix("# attendu : "))
                .map(|s| s.trim().to_string());
            Exemple {
                chemin,
                texte,
                attendu,
            }
        })
        .collect()
}

/// Exemple par prefixe de nom (`b06`).
pub fn exemple(prefixe: &str) -> Exemple {
    exemples()
        .into_iter()
        .find(|e| {
            e.chemin
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(prefixe)
        })
        .unwrap_or_else(|| panic!("exemple {prefixe} introuvable"))
}

/// Configuration de test (delai de securite pour ne jamais bloquer).
pub fn config(strategie: Strategie, graine: u64, workers: usize) -> Configuration {
    Configuration::constructeur()
        .strategie(strategie)
        .graine(graine)
        .workers(workers)
        .delai_max(Some(Duration::from_secs(60)))
        .construire()
}

/// Execute un texte source.
pub fn executer(texte: &str, configuration: &Configuration) -> Execution {
    Interpreteur::new()
        .executer_texte(texte, configuration)
        .unwrap_or_else(|d| panic!("programme invalide :\n{d}"))
}

/// Execute en enregistrant les evenements ; verifie la trace par rejeu
/// (re-execution de chaque reaction) et retourne l'execution et la trace.
pub fn executer_et_verifier(
    texte: &str,
    configuration: &Configuration,
) -> (Execution, Vec<Evenement>) {
    let interpreteur = Interpreteur::new();
    let programme = interpreteur
        .charger(&Source::new("test.chem", texte), None)
        .unwrap_or_else(|d| panic!("programme invalide :\n{d}"));
    let regles = programme.compile.regles.clone();
    let enregistreur = Arc::new(Enregistreur::new());
    let diffuseur = Diffuseur::new().avec(Arc::clone(&enregistreur));
    let execution = interpreteur.executer(programme, configuration, &diffuseur, None);
    let evenements = enregistreur.evenements();
    let texte_trace: String = evenements.iter().map(|e| e.vers_ligne() + "\n").collect();
    let rejeu = rejeu::rejouer(texte_trace.as_bytes(), Some(&regles))
        .unwrap_or_else(|e| panic!("trace incoherente : {e}\n{texte_trace}"));
    assert_eq!(
        rejeu.modele.racine_canonique().as_deref(),
        Some(execution.solution.to_string().as_str()),
        "le rejeu ne reconstruit pas la solution finale"
    );
    (execution, evenements)
}

/// Execute `f` dans un thread et echoue si elle ne termine pas a temps
/// (detection d'interblocage).
pub fn avec_delai<T: Send + 'static>(delai: Duration, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (envoi, reception) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = envoi.send(f());
    });
    reception
        .recv_timeout(delai)
        .expect("delai depasse : interblocage ou non-terminaison")
}

/// Test par proprietes minimal : `cas` tirages deterministes ; le message
/// d'echec donne la graine du cas pour le reproduire.
pub fn pour_tout(
    nom: &str,
    cas: u64,
    graine: u64,
    mut propriete: impl FnMut(&mut Alea) -> Result<(), String>,
) {
    for i in 0..cas {
        let graine_cas = Alea::deriver(graine, &[i]);
        let mut alea = Alea::new(graine_cas);
        if let Err(message) = propriete(&mut alea) {
            panic!("propriete '{nom}' fausse (cas {i}, graine {graine_cas}) : {message}");
        }
    }
}

/// Liste d'entiers aleatoires.
pub fn entiers(alea: &mut Alea, longueur_max: usize, min: i64, max: i64) -> Vec<i64> {
    let n = alea.indice(longueur_max + 1);
    (0..n).map(|_| alea.entre(min, max)).collect()
}

/// Ecrit une liste d'entiers dans la syntaxe des solutions.
pub fn liste(valeurs: &[i64]) -> String {
    valeurs
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Strategie et nombre de workers aleatoires.
pub fn configuration_aleatoire(alea: &mut Alea) -> Configuration {
    let strategie = if alea.probabilite(0.5) {
        Strategie::Aleatoire
    } else {
        Strategie::Ordonnee
    };
    config(strategie, alea.suivant(), 1 + alea.indice(4))
}
