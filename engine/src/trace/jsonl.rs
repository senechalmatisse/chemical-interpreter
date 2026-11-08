//! Observateur ecrivant la trace au format JSON Lines.

use super::{Evenement, Observateur};
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::{Mutex, PoisonError};

/// Ecrit un evenement par ligne dans un flux.
///
/// Chaque lot est ecrit sous un seul verrou : les lignes d'une reaction et de
/// ses effets (dissolution, injection) restent contigues dans le fichier.
pub struct TraceJsonl {
    sortie: Mutex<Box<dyn Write + Send>>,
    erreur: Mutex<Option<io::Error>>,
}

impl TraceJsonl {
    /// Trace vers un flux quelconque.
    pub fn new(sortie: Box<dyn Write + Send>) -> Self {
        Self {
            sortie: Mutex::new(sortie),
            erreur: Mutex::new(None),
        }
    }

    /// Trace vers un fichier (cree ou tronque).
    pub fn fichier(chemin: &Path) -> io::Result<Self> {
        let fichier = File::create(chemin)?;
        Ok(Self::new(Box::new(BufWriter::with_capacity(
            1 << 16,
            fichier,
        ))))
    }

    fn memoriser(&self, resultat: io::Result<()>) {
        if let Err(e) = resultat {
            let mut erreur = self.erreur.lock().unwrap_or_else(PoisonError::into_inner);
            if erreur.is_none() {
                *erreur = Some(e);
            }
        }
    }
}

impl Observateur for TraceJsonl {
    fn notifier(&self, evenement: &Evenement) {
        self.notifier_lot(std::slice::from_ref(evenement));
    }

    fn notifier_lot(&self, evenements: &[Evenement]) {
        let mut texte = String::with_capacity(evenements.len() * 96);
        for e in evenements {
            e.vers_json().ecrire(&mut texte);
            texte.push('\n');
        }
        let resultat = self
            .sortie
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .write_all(texte.as_bytes());
        self.memoriser(resultat);
    }

    fn terminer(&self) -> io::Result<()> {
        let resultat = self
            .sortie
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .flush();
        self.memoriser(resultat);
        match self
            .erreur
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
