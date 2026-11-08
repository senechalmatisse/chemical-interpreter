//! Visualisation console : etat de la solution a chaque etape, et
//! mode pas-a-pas interactif.

use super::modele::ModeleTrace;
use super::{Evenement, Observateur};
use crate::annulation::JetonAnnulation;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

struct EtatConsole {
    modele: ModeleTrace,
    sortie: Box<dyn Write + Send>,
    initial_affiche: bool,
    en_attente: bool,
}

impl EtatConsole {
    fn afficher_etat(&mut self) -> io::Result<()> {
        if let Some(texte) = self.modele.racine_canonique() {
            writeln!(self.sortie, "    {texte}")?;
        }
        for sol in self.modele.destinataires() {
            let valeurs: Vec<String> = self
                .modele
                .en_transit(sol)
                .iter()
                .map(|v| v.to_string())
                .collect();
            writeln!(
                self.sortie,
                "    en transit vers {sol} : {}",
                valeurs.join(", ")
            )?;
        }
        self.en_attente = false;
        Ok(())
    }

    fn valeurs(
        &self,
        sol: crate::solution::IdSolution,
        ids: &[crate::solution::IdMolecule],
    ) -> String {
        ids.iter()
            .map(|id| {
                self.modele
                    .valeur(sol, *id)
                    .map_or_else(|| format!("#{id}"), |v| v.to_string())
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn traiter(&mut self, e: &Evenement) -> io::Result<()> {
        let annexe = matches!(
            e,
            Evenement::Dissolution { .. }
                | Evenement::Injection { .. }
                | Evenement::Envoi { .. }
                | Evenement::Destruction { .. }
        );
        if !annexe && self.en_attente {
            self.afficher_etat()?;
        }
        if !self.initial_affiche && !matches!(e, Evenement::Debut { .. } | Evenement::Init { .. }) {
            writeln!(self.sortie, "etape 0 (etat initial)")?;
            self.afficher_etat()?;
            self.initial_affiche = true;
        }
        match e {
            Evenement::Reaction {
                n,
                w,
                sol,
                regle,
                entrees,
                entrees_sol,
                sorties,
                retiree,
                ..
            } => {
                let mut consommes = self.valeurs(*sol, entrees);
                for s in entrees_sol {
                    if !consommes.is_empty() {
                        consommes.push_str(", ");
                    }
                    consommes.push_str(&s.to_string());
                }
                if consommes.is_empty() {
                    consommes.push_str("rien");
                }
                let produits: Vec<String> = sorties.iter().map(|m| m.valeur.to_string()).collect();
                writeln!(
                    self.sortie,
                    "etape {n} : {regle} dans {sol} (worker {w}) : {consommes} -> {}",
                    if produits.is_empty() {
                        "rien".to_string()
                    } else {
                        produits.join(", ")
                    }
                )?;
                if *retiree {
                    writeln!(
                        self.sortie,
                        "    regle '{regle}' retiree de {sol} (usage unique)"
                    )?;
                }
                self.en_attente = true;
            }
            Evenement::Envoi { cible, sorties, .. } => {
                let v: Vec<String> = sorties.iter().map(|m| m.valeur.to_string()).collect();
                writeln!(
                    self.sortie,
                    "    envoi asynchrone de {} vers {cible}",
                    v.join(", ")
                )?;
                self.en_attente = true;
            }
            Evenement::Reception { sol, molecules, .. } => {
                let v: Vec<String> = molecules
                    .iter()
                    .map(|id| {
                        self.modele
                            .valeur_en_transit(*sol, *id)
                            .map_or_else(|| format!("#{id}"), |v| v.to_string())
                    })
                    .collect();
                writeln!(self.sortie, "reception dans {sol} : {}", v.join(", "))?;
                self.en_attente = true;
            }
            Evenement::Dissolution { sol, cible, .. } => {
                writeln!(self.sortie, "    dissolution de {cible} dans {sol}")?;
                self.en_attente = true;
            }
            Evenement::Injection { cible, sorties, .. } => {
                let v: Vec<String> = sorties.iter().map(|m| m.valeur.to_string()).collect();
                writeln!(
                    self.sortie,
                    "    injection de {} dans {cible}",
                    v.join(", ")
                )?;
                self.en_attente = true;
            }
            Evenement::Destruction { cible, .. } => {
                writeln!(self.sortie, "    destruction de {cible}")?;
                self.en_attente = true;
            }
            Evenement::Fin {
                reactions, motif, ..
            } => {
                writeln!(
                    self.sortie,
                    "fin : {reactions} reaction(s), arret par {motif}"
                )?;
            }
            _ => {}
        }
        if let Err(message) = self.modele.appliquer(e) {
            writeln!(self.sortie, "    (trace incoherente : {message})")?;
        }
        Ok(())
    }
}

/// Affiche la solution apres chaque etape (`--trace`).
pub struct TraceConsole {
    etat: Mutex<EtatConsole>,
}

impl TraceConsole {
    /// Console ecrivant dans le flux donne.
    pub fn new(sortie: Box<dyn Write + Send>) -> Self {
        Self {
            etat: Mutex::new(EtatConsole {
                modele: ModeleTrace::new(),
                sortie,
                initial_affiche: false,
                en_attente: false,
            }),
        }
    }
}

impl Observateur for TraceConsole {
    fn notifier(&self, evenement: &Evenement) {
        self.notifier_lot(std::slice::from_ref(evenement));
    }

    fn notifier_lot(&self, evenements: &[Evenement]) {
        let mut etat = self.etat.lock().unwrap_or_else(PoisonError::into_inner);
        for e in evenements {
            if etat.traiter(e).is_err() {
                return;
            }
        }
        if etat.en_attente {
            let _ = etat.afficher_etat();
        }
        let _ = etat.sortie.flush();
    }

    fn terminer(&self) -> io::Result<()> {
        self.etat
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .sortie
            .flush()
    }
}

/// Patron Decorateur : ajoute une pause interactive apres chaque reaction a
/// n'importe quel observateur (en pratique [`TraceConsole`]).
///
/// Commandes : Entree = etape suivante, `c` = continuer sans pause,
/// `q` = arreter l'execution.
pub struct PasAPas<O: Observateur> {
    interne: O,
    entree: Mutex<Box<dyn BufRead + Send>>,
    invite: Mutex<Box<dyn Write + Send>>,
    annulation: JetonAnnulation,
    sans_pause: AtomicBool,
}

impl<O: Observateur> PasAPas<O> {
    /// Decore `interne` ; lit les commandes sur `entree`, ecrit l'invite sur `invite`.
    pub fn new(
        interne: O,
        entree: Box<dyn BufRead + Send>,
        invite: Box<dyn Write + Send>,
        annulation: JetonAnnulation,
    ) -> Self {
        Self {
            interne,
            entree: Mutex::new(entree),
            invite: Mutex::new(invite),
            annulation,
            sans_pause: AtomicBool::new(false),
        }
    }

    fn pause(&self) {
        {
            let mut invite = self.invite.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = write!(invite, "[Entree] suivant, [c] continuer, [q] quitter > ");
            let _ = invite.flush();
        }
        let mut ligne = String::new();
        let lu = self
            .entree
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .read_line(&mut ligne);
        match (lu, ligne.trim()) {
            (Ok(0) | Err(_), _) => self.sans_pause.store(true, Ordering::Relaxed),
            (_, "c") => self.sans_pause.store(true, Ordering::Relaxed),
            (_, "q") => {
                self.annulation.demander();
                self.sans_pause.store(true, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

impl<O: Observateur> Observateur for PasAPas<O> {
    fn notifier(&self, evenement: &Evenement) {
        self.notifier_lot(std::slice::from_ref(evenement));
    }

    fn notifier_lot(&self, evenements: &[Evenement]) {
        self.interne.notifier_lot(evenements);
        let reaction = evenements
            .iter()
            .any(|e| matches!(e, Evenement::Reaction { .. }));
        if reaction && !self.sans_pause.load(Ordering::Relaxed) {
            self.pause();
        }
    }

    fn exige_immediatete(&self) -> bool {
        true
    }

    fn terminer(&self) -> io::Result<()> {
        self.interne.terminer()
    }
}
