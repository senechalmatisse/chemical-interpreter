//! Ligne de commande : sous-commandes et codes de retour.

mod commun;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn chem(arguments: &[&str], entree: Option<&str>) -> Output {
    let mut commande = Command::new(env!("CARGO_BIN_EXE_chem"));
    commande
        .args(arguments)
        .current_dir(commun::depot())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut processus = commande.spawn().expect("lancement de chem");
    {
        let mut stdin = processus.stdin.take().unwrap();
        if let Some(texte) = entree {
            stdin.write_all(texte.as_bytes()).unwrap();
        }
    }
    processus.wait_with_output().unwrap()
}

fn texte(octets: &[u8]) -> String {
    String::from_utf8_lossy(octets).to_string()
}

fn temporaire(nom: &str, contenu: &str) -> PathBuf {
    let repertoire = std::env::temp_dir().join(format!("chem-tests-{}", std::process::id()));
    std::fs::create_dir_all(&repertoire).unwrap();
    let chemin = repertoire.join(nom);
    std::fs::write(&chemin, contenu).unwrap();
    chemin
}

#[test]
fn run_affiche_la_solution_finale() {
    let sortie = chem(&["run", "exemples/b01_maximum.chem"], None);
    assert_eq!(sortie.status.code(), Some(0));
    assert_eq!(texte(&sortie.stdout), "{ 17 | max }\n");
    let sortie = chem(
        &[
            "run",
            "exemples/b06_dissolution.chem",
            "--workers",
            "4",
            "--stats",
        ],
        None,
    );
    assert_eq!(sortie.status.code(), Some(0));
    assert_eq!(texte(&sortie.stdout), "{ 21 | vider, max }\n");
    assert!(texte(&sortie.stderr).contains("dissolution"));
}

#[test]
fn codes_de_retour() {
    let invalide = chem(
        &[
            "run",
            "engine/tests/invalides/i04_point_virgule_oublie.chem",
        ],
        None,
    );
    assert_eq!(invalide.status.code(), Some(1));
    assert!(texte(&invalide.stderr).contains("erreur[E0201]"));
    assert!(texte(&invalide.stderr).contains("i04_point_virgule_oublie.chem:2:38"));

    let division = temporaire(
        "division.chem",
        "let d = replace x, y by x / y;\n{ 4, 0 | d }\n",
    );
    let execution = chem(
        &["run", division.to_str().unwrap(), "--strategie", "ordonnee"],
        None,
    );
    assert_eq!(execution.status.code(), Some(2));
    assert!(texte(&execution.stderr).contains("division par zero"));

    let boucle = temporaire("boucle.chem", "let r = replace x by x + 1;\n{ 0 | r }\n");
    let garde = chem(
        &["run", boucle.to_str().unwrap(), "--max-etapes", "25"],
        None,
    );
    assert_eq!(garde.status.code(), Some(3));
    assert_eq!(texte(&garde.stdout), "{ 25 | r }\n");
    let delai = chem(
        &["run", boucle.to_str().unwrap(), "--delai-max", "100ms"],
        None,
    );
    assert_eq!(delai.status.code(), Some(3));
    assert!(texte(&delai.stderr).contains("delai"));

    let usage = chem(
        &["run", "exemples/b01_maximum.chem", "--workers", "zero"],
        None,
    );
    assert_eq!(usage.status.code(), Some(1));
}

#[test]
fn check_et_fmt() {
    let check = chem(&["check", "exemples/b07_injection.chem"], None);
    assert_eq!(check.status.code(), Some(0));
    assert!(texte(&check.stdout).starts_with("ok : 2 regle(s)"));

    let fmt = chem(&["fmt", "exemples/b05_tri.chem"], None);
    assert_eq!(fmt.status.code(), Some(0));
    let normalise = texte(&fmt.stdout);
    assert!(
        normalise.contains(
            "let echanger = replace (i, a), (j, b) by (i, b), (j, a) if i < j and a > b;"
        )
    );
    // la forme normalisee est stable
    let chemin = temporaire("normalise.chem", &normalise);
    let refmt = chem(&["fmt", chemin.to_str().unwrap()], None);
    assert_eq!(texte(&refmt.stdout), normalise);
}

#[test]
fn trace_jsonl_et_rejeu_verifie() {
    let trace = temporaire("b07.jsonl", "");
    let run = chem(
        &[
            "run",
            "exemples/b07_injection.chem",
            "--workers",
            "2",
            "--trace-jsonl",
            trace.to_str().unwrap(),
        ],
        None,
    );
    assert_eq!(run.status.code(), Some(0));
    let contenu = std::fs::read_to_string(&trace).unwrap();
    assert!(
        contenu
            .lines()
            .all(|l| l.starts_with('{') && l.ends_with('}'))
    );
    assert!(contenu.contains("\"ev\":\"injection\""));
    let rejeu = chem(
        &[
            "replay",
            trace.to_str().unwrap(),
            "--programme",
            "exemples/b07_injection.chem",
        ],
        None,
    );
    assert_eq!(rejeu.status.code(), Some(0), "{}", texte(&rejeu.stderr));
    assert_eq!(texte(&rejeu.stdout), texte(&run.stdout));
    assert!(texte(&rejeu.stderr).contains("re-executee(s) et verifiee(s)"));

    // une trace falsifiee est refusee
    let falsifiee = contenu.replacen("\"v\":25", "\"v\":26", 1);
    let chemin = temporaire("falsifiee.jsonl", &falsifiee);
    let refus = chem(
        &[
            "replay",
            chemin.to_str().unwrap(),
            "--programme",
            "exemples/b07_injection.chem",
        ],
        None,
    );
    assert_eq!(refus.status.code(), Some(2), "{}", texte(&refus.stderr));
}

#[test]
fn trace_console_et_pas_a_pas() {
    let console = chem(
        &[
            "run",
            "exemples/b01_maximum.chem",
            "--strategie",
            "ordonnee",
            "--trace",
        ],
        None,
    );
    let sortie = texte(&console.stdout);
    assert!(sortie.contains("etape 0 (etat initial)"));
    assert!(sortie.contains("etape 4 : max dans s0"));
    assert!(sortie.ends_with("{ 17 | max }\n"));

    let quitter = chem(
        &["run", "exemples/b01_maximum.chem", "--pas-a-pas"],
        Some("\nq\n"),
    );
    assert_eq!(quitter.status.code(), Some(3), "{}", texte(&quitter.stderr));
    assert!(texte(&quitter.stderr).contains("interrompue"));

    let continuer = chem(
        &["run", "exemples/b01_maximum.chem", "--pas-a-pas"],
        Some("c\n"),
    );
    assert_eq!(continuer.status.code(), Some(0));
    assert!(texte(&continuer.stdout).contains("[Entree] suivant"));
}

#[test]
fn viz_sans_visualiseur_construit() {
    let trace = temporaire("viz.jsonl", "{}\n");
    let viz = chem(
        &[
            "viz",
            trace.to_str().unwrap(),
            "--dist",
            "/repertoire/inexistant",
            "--sans-navigateur",
        ],
        None,
    );
    assert_eq!(viz.status.code(), Some(1));
    assert!(texte(&viz.stderr).contains("npm run build"));
}
