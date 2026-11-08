//! Boite aux lettres d'une solution : injection asynchrone.
//!
//! Une regle de la solution parente peut envoyer des molecules a une
//! sous-solution directe designee par une reference (`s:ref`), meme si
//! celle-ci est en train de reagir sur un autre worker. L'envoi ne touche
//! pas au multi-ensemble de la cible (possede par le worker qui la fait
//! reagir) : il depose les molecules dans sa boite aux lettres, que la cible
//! releve au debut de chacune de ses passes et de chacun de ses tours.
//!
//! # Protocole
//!
//! Une sous-solution inerte peut etre a la fois la cible d'un envoi (par un
//! worker de la solution parente) et liee par un motif `s:sol` (par un autre
//! worker de la meme solution parente) pour etre dissoute ou detruite. Les
//! deux operations sont rendues exclusives sans attente :
//!
//! - l'expediteur **reserve** l'envoi ([`BoiteAuxLettres::reserver_envoi`]),
//!   ce qui echoue si la boite est fermee, puis **depose** les molecules
//!   ([`BoiteAuxLettres::deposer`]) ou annule ;
//! - la regle qui lie la sous-solution la **ferme**
//!   ([`BoiteAuxLettres::fermer_si_libre`]), ce qui echoue si du courrier
//!   attend ou si un envoi est en cours : la sous-solution n'est alors pas
//!   inerte et ne peut pas etre liee. Si la sous-solution survit a la
//!   reaction (conservation, injection synchrone), la boite est rouverte.
//!
//! Aucun verrou n'est tenu pendant une recherche de reactifs : chaque
//! operation est une courte section critique, aucune n'attend une autre
//! (pas d'interblocage possible).

use super::Molecule;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

#[derive(Debug, Default)]
struct EtatBoite {
    molecules: Vec<Molecule>,
    envois_en_cours: usize,
    fermee: bool,
}

/// Boite aux lettres d'une solution (partagee par `Arc` entre la solution
/// et les references que sa solution parente a construites).
#[derive(Debug, Default)]
pub struct BoiteAuxLettres {
    /// Indicateur rapide (lu sans verrou) : du courrier attend.
    courrier: AtomicBool,
    etat: Mutex<EtatBoite>,
}

impl BoiteAuxLettres {
    /// Boite vide et ouverte.
    pub fn new() -> Self {
        Self::default()
    }

    fn etat(&self) -> MutexGuard<'_, EtatBoite> {
        self.etat.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Reserve un envoi ; faux si la boite est fermee (sous-solution liee,
    /// dissoute ou detruite). Une reservation reussie doit etre suivie de
    /// [`Self::deposer`] ou de [`Self::annuler_envoi`].
    pub fn reserver_envoi(&self) -> bool {
        let mut etat = self.etat();
        if etat.fermee {
            return false;
        }
        etat.envois_en_cours += 1;
        true
    }

    /// Annule une reservation sans rien deposer.
    pub fn annuler_envoi(&self) {
        let mut etat = self.etat();
        debug_assert!(etat.envois_en_cours > 0, "annulation sans reservation");
        etat.envois_en_cours = etat.envois_en_cours.saturating_sub(1);
    }

    /// Depose des molecules et termine la reservation correspondante.
    pub fn deposer(&self, molecules: impl IntoIterator<Item = Molecule>) {
        let mut etat = self.etat();
        debug_assert!(etat.envois_en_cours > 0, "depot sans reservation");
        etat.envois_en_cours = etat.envois_en_cours.saturating_sub(1);
        etat.molecules.extend(molecules);
        if !etat.molecules.is_empty() {
            self.courrier.store(true, Ordering::Release);
        }
    }

    /// Retire et retourne le courrier en attente (vide le plus souvent, sans
    /// prise de verrou).
    pub fn relever(&self) -> Vec<Molecule> {
        if !self.a_du_courrier() {
            return Vec::new();
        }
        let mut etat = self.etat();
        self.courrier.store(false, Ordering::Release);
        std::mem::take(&mut etat.molecules)
    }

    /// Ferme la boite si aucun courrier n'attend et qu'aucun envoi n'est en
    /// cours : la sous-solution peut alors etre liee. Faux sinon (elle n'est
    /// pas inerte) ou si la boite est deja fermee.
    pub fn fermer_si_libre(&self) -> bool {
        let mut etat = self.etat();
        if etat.fermee || etat.envois_en_cours > 0 || !etat.molecules.is_empty() {
            return false;
        }
        etat.fermee = true;
        true
    }

    /// Rouvre une boite fermee par [`Self::fermer_si_libre`].
    pub fn rouvrir(&self) {
        self.etat().fermee = false;
    }

    /// Vrai si du courrier attend d'etre releve.
    pub fn a_du_courrier(&self) -> bool {
        self.courrier.load(Ordering::Acquire)
    }

    /// Vrai si la boite est fermee.
    pub fn est_fermee(&self) -> bool {
        self.etat().fermee
    }

    /// Nombre de molecules en attente.
    pub fn nombre(&self) -> usize {
        self.etat().molecules.len()
    }

    /// Nouvelle boite, ouverte, contenant une copie du courrier en attente
    /// (copie independante d'une solution).
    pub fn copie(&self) -> Self {
        let molecules = self.etat().molecules.clone();
        Self {
            courrier: AtomicBool::new(!molecules.is_empty()),
            etat: Mutex::new(EtatBoite {
                molecules,
                ..EtatBoite::default()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluateur::Valeur;
    use crate::solution::IdMolecule;
    use std::sync::Arc;

    fn m(id: u64) -> Molecule {
        Molecule {
            id: IdMolecule(id),
            valeur: Valeur::Entier(id as i64),
        }
    }

    #[test]
    fn envoi_puis_releve() {
        let b = BoiteAuxLettres::new();
        assert!(!b.a_du_courrier());
        assert!(b.relever().is_empty());
        assert!(b.reserver_envoi());
        b.deposer([m(1), m(2)]);
        assert!(b.a_du_courrier());
        assert_eq!(b.nombre(), 2);
        assert_eq!(b.copie().nombre(), 2);
        assert_eq!(b.relever(), vec![m(1), m(2)]);
        assert!(!b.a_du_courrier());
        assert!(b.relever().is_empty());
    }

    #[test]
    fn fermeture_exclusive_avec_les_envois() {
        let b = BoiteAuxLettres::new();
        // envoi en cours : la sous-solution n'est pas liable
        assert!(b.reserver_envoi());
        assert!(!b.fermer_si_libre());
        b.annuler_envoi();
        assert!(b.fermer_si_libre());
        // fermee : aucun envoi possible, pas de seconde fermeture
        assert!(b.est_fermee());
        assert!(!b.reserver_envoi());
        assert!(!b.fermer_si_libre());
        b.rouvrir();
        assert!(b.reserver_envoi());
        b.deposer([m(3)]);
        // courrier en attente : pas inerte, pas liable
        assert!(!b.fermer_si_libre());
        b.relever();
        assert!(b.fermer_si_libre());
    }

    #[test]
    fn concurrence_sans_perte() {
        // Des expediteurs et un releveur concurrents : chaque molecule
        // deposee est relevee exactement une fois.
        let b = Arc::new(BoiteAuxLettres::new());
        let recues = std::thread::scope(|portee| {
            for t in 0..4u64 {
                let b = Arc::clone(&b);
                portee.spawn(move || {
                    for k in 0..500 {
                        assert!(b.reserver_envoi());
                        b.deposer([m(t * 1000 + k)]);
                    }
                });
            }
            let releveur = portee.spawn(|| {
                let mut recues = Vec::new();
                while recues.len() < 2000 {
                    recues.extend(b.relever());
                    std::thread::yield_now();
                }
                recues
            });
            releveur.join().unwrap()
        });
        let mut ids: Vec<u64> = recues.iter().map(|m| m.id.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 2000);
        assert!(!b.a_du_courrier());
    }
}
