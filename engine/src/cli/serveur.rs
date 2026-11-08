//! Serveur HTTP minimal pour `chem viz` : sert le visualiseur construit et
//! la trace (`/trace.jsonl`), en local uniquement.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

fn type_mime(chemin: &Path) -> &'static str {
    match chemin.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("jsonl") => "application/x-ndjson; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Resout un chemin d'URL dans `racine` en refusant toute sortie du repertoire.
pub fn resoudre(racine: &Path, chemin_url: &str) -> Option<PathBuf> {
    let chemin = chemin_url.split(['?', '#']).next().unwrap_or("/");
    let mut resultat = racine.to_path_buf();
    for composant in Path::new(chemin.trim_start_matches('/')).components() {
        match composant {
            Component::Normal(c) => resultat.push(c),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if resultat.is_dir() {
        resultat.push("index.html");
    }
    Some(resultat)
}

fn repondre(flux: &mut TcpStream, statut: &str, mime: &str, corps: &[u8]) -> io::Result<()> {
    write!(
        flux,
        "HTTP/1.1 {statut}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        corps.len()
    )?;
    flux.write_all(corps)
}

fn traiter(mut flux: TcpStream, dist: &Path, trace: &Path) -> io::Result<()> {
    let mut lecteur = BufReader::new(flux.try_clone()?);
    let mut ligne = String::new();
    lecteur.read_line(&mut ligne)?;
    // Lit les en-tetes jusqu'a la ligne vide : fermer la connexion avec des
    // donnees non lues provoquerait une reinitialisation cote navigateur.
    let mut entete = String::new();
    while lecteur.read_line(&mut entete)? > 0 && !entete.trim().is_empty() {
        entete.clear();
    }
    let mut morceaux = ligne.split_whitespace();
    let (methode, chemin) = (
        morceaux.next().unwrap_or(""),
        morceaux.next().unwrap_or("/"),
    );
    if methode != "GET" {
        return repondre(
            &mut flux,
            "405 Method Not Allowed",
            "text/plain",
            b"GET uniquement",
        );
    }
    let fichier = if chemin.split('?').next() == Some("/trace.jsonl") {
        Some(trace.to_path_buf())
    } else {
        resoudre(dist, chemin)
    };
    match fichier.and_then(|f| fs::read(&f).ok().map(|c| (f, c))) {
        Some((f, contenu)) => repondre(&mut flux, "200 OK", type_mime(&f), &contenu),
        None => repondre(&mut flux, "404 Not Found", "text/plain", b"introuvable"),
    }
}

fn ouvrir_navigateur(url: &str) {
    let commande = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    let _ = std::process::Command::new(commande.0)
        .args(commande.1)
        .spawn();
}

/// Sert le visualiseur jusqu'a l'interruption du processus.
pub fn servir(
    dist: &Path,
    trace: &Path,
    port: u16,
    ouvrir: bool,
    sortie: &mut impl Write,
) -> io::Result<()> {
    let ecoute = TcpListener::bind(("127.0.0.1", port))?;
    let adresse = ecoute.local_addr()?;
    let url = format!("http://{adresse}/?trace=/trace.jsonl");
    writeln!(
        sortie,
        "visualiseur disponible sur {url} (Ctrl-C pour arreter)"
    )?;
    sortie.flush()?;
    if ouvrir {
        ouvrir_navigateur(&url);
    }
    for connexion in ecoute.incoming() {
        match connexion {
            Ok(flux) => {
                let _ = traiter(flux, dist, trace);
            }
            Err(e) => writeln!(sortie, "connexion refusee : {e}")?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_surs() {
        let racine = std::env::temp_dir();
        assert!(resoudre(&racine, "/../etc/passwd").is_none());
        assert!(resoudre(&racine, "/a/../../b").is_none());
        assert_eq!(
            resoudre(&racine, "/assets/x.js?v=1"),
            Some(racine.join("assets").join("x.js"))
        );
        assert_eq!(
            type_mime(Path::new("a.js")),
            "text/javascript; charset=utf-8"
        );
    }
}
