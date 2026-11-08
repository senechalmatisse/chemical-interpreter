//! Les implementations du paquet se comportent comme celles de la
//! bibliotheque : memes arguments analyses, memes lignes de trace, memes
//! solutions finales, diagnostics complets. Chaque partie ne compile que si
//! sa fonctionnalite est active.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Racine du depot (contient `exemples/`).
fn depot() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Programme d'exemple et resultat attendu (`# attendu : ...`).
struct Exemple {
    chemin: PathBuf,
    texte: String,
    attendu: Option<String>,
}

/// Programmes de `exemples/` (hors sous-repertoires), tries par nom.
fn exemples() -> Vec<Exemple> {
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

/// Lance l'executable du paquet depuis la racine du depot.
fn chem(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_chem"))
        .args(arguments)
        .current_dir(depot())
        .output()
        .expect("lancement de chem")
}

fn texte(octets: &[u8]) -> String {
    String::from_utf8_lossy(octets).to_string()
}

#[test]
fn executable_complet() {
    let sortie = chem(&["--version"]);
    assert_eq!(sortie.status.code(), Some(0));
    let version = texte(&sortie.stdout);
    for (active, nom) in [
        (cfg!(feature = "clap"), "clap"),
        (cfg!(feature = "miette"), "miette"),
        (cfg!(feature = "serde"), "serde_json"),
        (cfg!(feature = "rayon"), "rayon"),
    ] {
        assert_eq!(version.contains(nom), active, "{nom} :\n{version}");
    }

    let sortie = chem(&["run", "exemples/b01_maximum.chem"]);
    assert_eq!(sortie.status.code(), Some(0), "{}", texte(&sortie.stderr));
    assert_eq!(texte(&sortie.stdout), "{ 17 | max }\n");

    let trace =
        std::env::temp_dir().join(format!("chem-integrations-{}.jsonl", std::process::id()));
    let trace = trace.to_str().unwrap();
    let sortie = chem(&[
        "run",
        "exemples/b06_dissolution.chem",
        "--workers",
        "4",
        "--seed",
        "3",
        "--trace-jsonl",
        trace,
    ]);
    assert_eq!(sortie.status.code(), Some(0), "{}", texte(&sortie.stderr));
    assert_eq!(texte(&sortie.stdout), "{ 21 | vider, max }\n");
    let sortie = chem(&[
        "replay",
        trace,
        "--programme",
        "exemples/b06_dissolution.chem",
    ]);
    assert_eq!(sortie.status.code(), Some(0), "{}", texte(&sortie.stderr));
    assert!(texte(&sortie.stdout).contains("{ 21 | vider, max }"));
    let _ = std::fs::remove_file(trace);

    let sortie = chem(&[
        "check",
        "engine/tests/invalides/i04_point_virgule_oublie.chem",
    ]);
    assert_eq!(sortie.status.code(), Some(1));
    assert!(
        texte(&sortie.stderr).contains("E0201"),
        "{}",
        texte(&sortie.stderr)
    );

    let sortie = chem(&["run", "exemples/b01_maximum.chem", "--inconnue"]);
    assert_eq!(sortie.status.code(), Some(1));
    assert!(
        texte(&sortie.stderr).contains("option inconnue '--inconnue'"),
        "{}",
        texte(&sortie.stderr)
    );
}

#[cfg(feature = "clap")]
mod arguments {
    use chem::cli::arguments::{AnalyseurArguments, AnalyseurInterne, DefinitionCommande};
    use chem::cli::{CHECK, REPLAY, RUN, VIZ};
    use chem_integrations::arguments::AnalyseurClap;

    fn comparer(definition: &DefinitionCommande, arguments: &[&str]) -> Result<(), String> {
        let arguments: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
        let interne = AnalyseurInterne.analyser(definition, &arguments);
        let clap = AnalyseurClap.analyser(definition, &arguments);
        match (&interne, &clap) {
            (Ok(_), Ok(_)) if interne == clap => Ok(()),
            (Err(a), Err(b)) if a == b => Err(a.clone()),
            _ => panic!(
                "chem {} {arguments:?} :\n  interne : {interne:?}\n  clap    : {clap:?}",
                definition.nom
            ),
        }
    }

    #[test]
    fn memes_arguments_acceptes() {
        let cas: &[(&DefinitionCommande, &[&str])] = &[
            (&RUN, &["p.chem"]),
            (&RUN, &["p.chem", "--seed", "42"]),
            (&RUN, &["--seed=42", "p.chem"]),
            (&RUN, &["p.chem", "--seed", "-1"]),
            (&RUN, &["p.chem", "--seed", "1", "--seed", "2"]),
            (&RUN, &["p.chem", "--seed="]),
            (&RUN, &["p.chem", "--trace-jsonl", "-"]),
            (&RUN, &["p.chem", "--trace-jsonl", "--stats"]),
            (&RUN, &["-", "--stats"]),
            (
                &RUN,
                &[
                    "p.chem",
                    "--strategie",
                    "ordonnee",
                    "--workers",
                    "4",
                    "--max-etapes",
                    "10",
                    "--delai-max",
                    "1.5s",
                    "--max-molecules",
                    "aucune",
                    "--trace",
                    "--pas-a-pas",
                    "--stats",
                    "--sans-index",
                    "--sans-partition",
                ],
            ),
            (&RUN, &["--help"]),
            (&RUN, &["-h", "p.chem"]),
            (&RUN, &["--help", "a", "b"]),
            (&CHECK, &["p.chem"]),
            (&REPLAY, &["t.jsonl", "--programme", "p.chem"]),
            (&VIZ, &["t.jsonl", "--port=0", "--sans-navigateur"]),
        ];
        for (definition, arguments) in cas {
            assert_eq!(comparer(definition, arguments), Ok(()));
        }
    }

    #[test]
    fn memes_erreurs() {
        let cas: &[(&DefinitionCommande, &[&str], &str)] = &[
            (&RUN, &[], "argument manquant : <fichier.chem>"),
            (&RUN, &["a.chem", "b.chem"], "argument inattendu 'b.chem'"),
            (
                &RUN,
                &["a.chem", "--stats", "b.chem"],
                "argument inattendu 'b.chem'",
            ),
            (
                &RUN,
                &["p.chem", "--seed"],
                "l'option '--seed' attend une valeur <N>",
            ),
            (
                &RUN,
                &["p.chem", "--stats=oui"],
                "l'option '--stats' n'attend pas de valeur",
            ),
            (
                &RUN,
                &["p.chem", "--inconnue"],
                "option inconnue '--inconnue' pour 'chem run'",
            ),
            (
                &CHECK,
                &["p.chem", "--seed", "1"],
                "option inconnue '--seed' pour 'chem check'",
            ),
            (&RUN, &["p.chem", "-x"], "option inconnue '-x'"),
        ];
        for (definition, arguments, debut) in cas {
            let owned: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
            let interne = AnalyseurInterne.analyser(definition, &owned).unwrap_err();
            let clap = AnalyseurClap.analyser(definition, &owned).unwrap_err();
            assert!(
                interne.starts_with(debut),
                "interne {arguments:?} : {interne}"
            );
            assert!(clap.starts_with(debut), "clap {arguments:?} : {clap}");
        }
    }
}

#[cfg(feature = "serde")]
mod trace {
    use super::exemples;
    use chem::Interpreteur;
    use chem::ordonnanceur::{Configuration, Strategie};
    use chem::rejeu;
    use chem::source::Source;
    use chem::trace::{Diffuseur, Enregistreur, FormatInterne, FormatTrace};
    use chem_integrations::trace::FormatSerde;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn memes_lignes_et_meme_rejeu() {
        let mut verifies = 0;
        for exemple in exemples() {
            let Some(attendu) = &exemple.attendu else {
                continue;
            };
            let interpreteur = Interpreteur::new();
            let programme = interpreteur
                .charger(&Source::new("test.chem", exemple.texte.clone()), None)
                .unwrap_or_else(|d| panic!("{} : {d}", exemple.chemin.display()));
            let regles = programme.compile.regles.clone();
            let enregistreur = Arc::new(Enregistreur::new());
            let diffuseur = Diffuseur::new().avec(Arc::clone(&enregistreur));
            let configuration = Configuration::constructeur()
                .strategie(Strategie::Aleatoire)
                .graine(3)
                .workers(2)
                .delai_max(Some(Duration::from_secs(60)))
                .construire();
            let execution = interpreteur.executer(programme, &configuration, &diffuseur, None);
            assert_eq!(&execution.solution.to_string(), attendu);

            let mut lignes_interne = Vec::new();
            let mut lignes_serde = Vec::new();
            for evenement in enregistreur.evenements() {
                FormatInterne
                    .ecrire(&evenement, &mut lignes_interne)
                    .unwrap();
                FormatSerde.ecrire(&evenement, &mut lignes_serde).unwrap();
            }
            let texte_interne = String::from_utf8(lignes_interne).unwrap();
            assert_eq!(
                String::from_utf8(lignes_serde.clone()).unwrap(),
                texte_interne,
                "{}",
                exemple.chemin.display()
            );
            for ligne in texte_interne.lines() {
                assert_eq!(
                    FormatSerde.lire(ligne),
                    FormatInterne.lire(ligne),
                    "{} : {ligne}",
                    exemple.chemin.display()
                );
            }
            let rejeu = rejeu::rejouer_avec_format(&lignes_serde[..], Some(&regles), &FormatSerde)
                .unwrap_or_else(|e| panic!("{} : {e}", exemple.chemin.display()));
            assert_eq!(
                rejeu.modele.racine_canonique().as_deref(),
                Some(attendu.as_str())
            );
            verifies += 1;
        }
        assert!(verifies >= 8);
    }
}

#[cfg(feature = "rayon")]
mod parallelisme {
    use super::exemples;
    use chem::Interpreteur;
    use chem::ordonnanceur::{Configuration, Strategie};
    use chem::source::Source;
    use chem::trace::json::Json;
    use chem::trace::{Diffuseur, Enregistreur};
    use chem_integrations::parallelisme::FabriqueRayon;
    use std::sync::Arc;
    use std::time::Duration;

    fn configuration(graine: u64, workers: usize) -> Configuration {
        Configuration::constructeur()
            .strategie(Strategie::Aleatoire)
            .graine(graine)
            .workers(workers)
            .delai_max(Some(Duration::from_secs(60)))
            .construire()
    }

    #[test]
    fn exemples_avec_rayon() {
        let interpreteur = Interpreteur::new().avec_parallelisme(Arc::new(FabriqueRayon));
        let mut verifies = 0;
        for exemple in exemples() {
            let Some(attendu) = &exemple.attendu else {
                continue;
            };
            for workers in [1, 2, 4] {
                for graine in 1..=3 {
                    let execution = interpreteur
                        .executer_texte(&exemple.texte, &configuration(graine, workers))
                        .unwrap_or_else(|d| panic!("{} : {d}", exemple.chemin.display()));
                    assert!(execution.est_inerte(), "{}", exemple.chemin.display());
                    assert_eq!(
                        &execution.solution.to_string(),
                        attendu,
                        "{} avec {workers} workers, graine {graine}",
                        exemple.chemin.display()
                    );
                }
            }
            verifies += 1;
        }
        assert!(verifies >= 8);
    }

    /// Trace sans les instants (seule difference attendue entre deux pools).
    fn trace_sans_instants(interpreteur: &Interpreteur, texte: &str) -> Vec<String> {
        let programme = interpreteur
            .charger(&Source::new("test.chem", texte.to_string()), None)
            .expect("programme valide");
        let enregistreur = Arc::new(Enregistreur::new());
        let diffuseur = Diffuseur::new().avec(Arc::clone(&enregistreur));
        interpreteur.executer(programme, &configuration(5, 1), &diffuseur, None);
        enregistreur
            .evenements()
            .iter()
            .map(|evenement| match evenement.vers_json() {
                Json::Objet(champs) => {
                    Json::Objet(champs.into_iter().filter(|(cle, _)| cle != "t").collect())
                        .to_string()
                }
                autre => autre.to_string(),
            })
            .collect()
    }

    #[test]
    fn un_worker_meme_execution_que_le_pool_standard() {
        let standard = Interpreteur::new();
        let rayon = Interpreteur::new().avec_parallelisme(Arc::new(FabriqueRayon));
        for exemple in exemples() {
            if exemple.attendu.is_none() {
                continue;
            }
            assert_eq!(
                trace_sans_instants(&rayon, &exemple.texte),
                trace_sans_instants(&standard, &exemple.texte),
                "{}",
                exemple.chemin.display()
            );
        }
    }
}

#[cfg(feature = "miette")]
mod diagnostics {
    use super::depot;
    use chem::Interpreteur;
    use chem::diagnostic::RenduDiagnostic;
    use chem::source::Source;
    use chem_integrations::diagnostics::RenduMiette;

    /// Caracteres ASCII visibles seulement : Miette replie les lignes
    /// longues et encadre l'extrait de caracteres graphiques.
    fn compact(texte: &str) -> String {
        texte.chars().filter(char::is_ascii_graphic).collect()
    }

    #[test]
    fn diagnostics_des_programmes_invalides_complets() {
        let repertoire = depot().join("engine/tests/invalides");
        let mut fichiers: Vec<_> = std::fs::read_dir(&repertoire)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "chem"))
            .collect();
        fichiers.sort();
        assert!(fichiers.len() >= 10);
        let rendu = RenduMiette::sans_couleurs();
        for chemin in fichiers {
            let texte = std::fs::read_to_string(&chemin).unwrap();
            let source = Source::new(chemin.display().to_string(), texte);
            let Err(diagnostics) = Interpreteur::new().charger(&source, Some(1_000_000)) else {
                panic!("{} devrait etre rejete", chemin.display());
            };
            for diagnostic in diagnostics.iter() {
                let sortie = rendu.rendre(diagnostic, &source);
                let compacte = compact(&sortie);
                let mut attendus = vec![diagnostic.code.to_string(), diagnostic.message.clone()];
                attendus.extend(diagnostic.etiquettes.iter().map(|e| e.message.clone()));
                attendus.extend(diagnostic.aide.clone());
                for attendu in attendus {
                    assert!(
                        compacte.contains(&compact(&attendu)),
                        "{} : '{attendu}' absent de :\n{sortie}",
                        chemin.display()
                    );
                }
            }
        }
    }
}
