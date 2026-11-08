//! Ordonnanceur : execution d'une solution (imbriquee) jusqu'a l'inertie.
//!
//! # Algorithme
//!
//! `stabiliser(S)` repete des *tours* jusqu'a l'inertie de `S` :
//!
//! 1. les sous-solutions actives de `S` sont extraites ;
//! 2. en parallele (`joindre`) : chaque sous-solution active est stabilisee
//!    recursivement (une tache par sous-solution, EF-S2) **et** `S` reagit
//!    sur ses molecules et ses sous-solutions inertes (EF-S3) ;
//! 3. les sous-solutions sont reintegrees ;
//! 4. si aucune sous-solution n'etait active et qu'aucune reaction n'a eu
//!    lieu, `S` est inerte (detection exacte, EF-S6, EF-D4).
//!
//! Les deux branches du point 2 portent sur des donnees disjointes : aucune
//! sous-solution ne peut etre dissoute ou recevoir une injection synchrone
//! pendant qu'elle reagit.
//!
//! # Injection asynchrone
//!
//! Seule exception, voulue : une regle de `S` peut envoyer des molecules a
//! une sous-solution referencee (`s:ref`), y compris pendant qu'elle reagit.
//! Les references (ports) sont construites au debut du tour, avant
//! l'extraction des sous-solutions actives ; l'envoi passe par la boite aux
//! lettres de la cible (voir [`crate::solution::BoiteAuxLettres`]), relevee
//! par la cible au debut de chacun de ses tours et de chacune de ses passes.
//! Une sous-solution qui a du courrier n'est pas inerte : elle est extraite
//! comme active au tour suivant de `S` (qui a forcement lieu, puisque
//! l'envoi etait une reaction de `S`). L'inertie reste donc exacte.
//!
//! # Regles a usage unique et sans reactif
//!
//! Une activation `replace-one` porte un drapeau atomique, partage par tous
//! les lots de la phase : une seule reaction peut le lever. Apres la phase,
//! les activations utilisees sont retirees de la solution. Une regle sans
//! reactif est tentee une fois par passe, meme dans une solution vide.
//!
//! La phase de reaction de `S` est elle-meme parallele : ses
//! elements sont repartis en lots (au plus un par worker) qui reagissent
//! simultanement ; chaque lot possede ses molecules, la reservation des
//! reactifs est donc atomique par construction. Une fois les lots
//! localement inertes, ils sont fusionnes deux a deux et le traitement
//! reprend, jusqu'a un lot unique dont la passe exhaustive finale garantit
//! l'inertie. Chaque reaction d'un lot etant valide dans la solution
//! entiere, toute execution parallele correspond a une execution
//! sequentielle possible (EF-S5). Avec un seul worker, il n'y a qu'un lot et
//! qu'un thread : c'est le mode sequentiel.

mod configuration;
mod pool;

pub use configuration::{
    Configuration, ConstructeurConfiguration, Limites, MAX_MOLECULES_DEFAUT, Strategie,
};
pub use pool::Pool;

use crate::alea::Alea;
use crate::annulation::JetonAnnulation;
use crate::moteur::{Arret, Compteurs, ContexteExecution, GardeFous, Lot, Reacteur};
use crate::solution::{Element, GenerateurIds, IdSolution, Port, Solution};
use crate::trace::{Diffuseur, Evenement};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// En dessous de cette taille, une solution n'est pas partitionnee.
pub const TAILLE_MIN_LOT: usize = 64;

/// Issue d'une execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    /// La solution principale est inerte.
    Inerte,
    /// Arret avant l'inertie (garde-fou, interruption ou erreur).
    Arretee(Arret),
}

/// Statistiques d'une execution (EF-I7).
#[derive(Debug, Clone, Default)]
pub struct Statistiques {
    /// Compteurs de reactions, de passes, de dissolutions...
    pub compteurs: Compteurs,
    /// Tours (solutions imbriquees).
    pub tours: u64,
    /// Molecules au depart.
    pub taille_initiale: u64,
    /// Molecules a la fin.
    pub taille_finale: u64,
    /// Solutions au depart.
    pub solutions_initiales: u64,
    /// Solutions a la fin.
    pub solutions_finales: u64,
    /// Duree de l'execution (hors analyse et compilation).
    pub duree: Duration,
    /// Graine utilisee.
    pub graine: u64,
    /// Strategie utilisee.
    pub strategie: Strategie,
    /// Workers utilises.
    pub workers: usize,
}

impl fmt::Display for Statistiques {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = &self.compteurs;
        writeln!(f, "statistiques :")?;
        writeln!(
            f,
            "  graine         : {} (strategie {}, {} worker(s))",
            self.graine, self.strategie, self.workers
        )?;
        writeln!(f, "  reactions      : {}", c.reactions)?;
        let par_regle: Vec<String> = c
            .par_regle
            .iter()
            .map(|(k, v)| format!("{k} = {v}"))
            .collect();
        writeln!(
            f,
            "  par regle      : {}",
            if par_regle.is_empty() {
                "-".into()
            } else {
                par_regle.join(", ")
            }
        )?;
        let par_worker: Vec<String> = c
            .par_worker
            .iter()
            .map(|(k, v)| format!("w{k} = {v}"))
            .collect();
        writeln!(
            f,
            "  par worker     : {}",
            if par_worker.is_empty() {
                "-".into()
            } else {
                par_worker.join(", ")
            }
        )?;
        writeln!(
            f,
            "  sous-solutions : {} dissolution(s), {} injection(s), {} destruction(s)",
            c.dissolutions, c.injections, c.destructions
        )?;
        if c.envois > 0 || c.receptions > 0 {
            writeln!(
                f,
                "  asynchrone     : {} molecule(s) envoyee(s), {} recue(s)",
                c.envois, c.receptions
            )?;
        }
        if c.regles_retirees > 0 {
            writeln!(
                f,
                "  usage unique   : {} regle(s) appliquee(s) puis retiree(s)",
                c.regles_retirees
            )?;
        }
        writeln!(f, "  passes, tours  : {}, {}", c.passes, self.tours)?;
        writeln!(
            f,
            "  molecules      : {} -> {}",
            self.taille_initiale, self.taille_finale
        )?;
        writeln!(
            f,
            "  solutions      : {} -> {}",
            self.solutions_initiales, self.solutions_finales
        )?;
        write!(
            f,
            "  duree          : {:.3} ms",
            self.duree.as_secs_f64() * 1000.0
        )
    }
}

/// Resultat d'une execution.
#[derive(Debug, Clone)]
pub struct Execution {
    /// Solution finale (ou au moment de l'arret).
    pub solution: Solution,
    /// Issue.
    pub issue: Issue,
    /// Statistiques.
    pub statistiques: Statistiques,
}

impl Execution {
    /// Vrai si l'execution a atteint l'inertie.
    pub fn est_inerte(&self) -> bool {
        self.issue == Issue::Inerte
    }
}

/// Ordonnanceur d'une execution.
pub struct Ordonnanceur<'a> {
    configuration: &'a Configuration,
    diffuseur: &'a Diffuseur,
    annulation: Option<JetonAnnulation>,
    programme: String,
}

impl<'a> Ordonnanceur<'a> {
    /// Ordonnanceur pour une configuration et un diffuseur d'evenements.
    pub fn new(configuration: &'a Configuration, diffuseur: &'a Diffuseur) -> Self {
        Self {
            configuration,
            diffuseur,
            annulation: None,
            programme: String::new(),
        }
    }

    /// Jeton permettant d'interrompre l'execution.
    pub fn avec_annulation(mut self, jeton: JetonAnnulation) -> Self {
        self.annulation = Some(jeton);
        self
    }

    /// Nom du programme (repris dans la trace).
    pub fn avec_programme(mut self, nom: impl Into<String>) -> Self {
        self.programme = nom.into();
        self
    }

    /// Execute la solution jusqu'a l'inertie ou jusqu'a un arret.
    pub fn executer(&self, mut solution: Solution, ids: &GenerateurIds) -> Execution {
        let horloge = Instant::now();
        let config = self.configuration;
        let taille_initiale = solution.nombre_molecules();
        let solutions_initiales = solution.nombre_solutions();
        let limites = config.limites();
        let garde = GardeFous::new(
            limites.max_etapes,
            limites.delai_max,
            limites.max_molecules,
            taille_initiale,
            self.annulation.clone(),
            horloge,
        );
        let ctx = ContexteExecution::new(
            garde,
            ids,
            self.diffuseur,
            config.strategie() == Strategie::Aleatoire,
            horloge,
        );
        if self.diffuseur.est_actif() {
            self.publier_debut(&solution, &ctx);
        }
        let pool = Pool::new(config.workers());
        let moteur = Moteur {
            ctx: &ctx,
            pool: &pool,
            graine: config.graine(),
            workers: config.workers(),
        };
        let resultat = pool.installer(|| moteur.stabiliser(&mut solution, config.workers()));
        // Apres un arret, du courrier peut rester en transit : il rejoint ses
        // destinataires pour qu'aucune molecule ne soit perdue (a l'inertie,
        // toutes les boites sont deja vides).
        moteur.relever_tout(&mut solution);
        let issue = match resultat {
            Ok(()) => Issue::Inerte,
            Err(arret) => Issue::Arretee(ctx.garde().cause().unwrap_or(arret)),
        };
        let compteurs = ctx.compteurs();
        if self.diffuseur.est_actif() {
            self.diffuseur.diffuser(&Evenement::Fin {
                t: ctx.temps(),
                reactions: compteurs.reactions,
                inerte: issue == Issue::Inerte,
                motif: match &issue {
                    Issue::Inerte => "inertie".to_string(),
                    Issue::Arretee(a) => a.motif().to_string(),
                },
            });
        }
        let statistiques = Statistiques {
            compteurs,
            tours: ctx.tours(),
            taille_initiale,
            taille_finale: solution.nombre_molecules(),
            solutions_initiales,
            solutions_finales: solution.nombre_solutions(),
            duree: horloge.elapsed(),
            graine: config.graine(),
            strategie: config.strategie(),
            workers: config.workers(),
        };
        Execution {
            solution,
            issue,
            statistiques,
        }
    }

    fn publier_debut(&self, solution: &Solution, ctx: &ContexteExecution<'_>) {
        self.diffuseur.diffuser(&Evenement::Debut {
            graine: self.configuration.graine(),
            strategie: self.configuration.strategie().nom().to_string(),
            workers: self.configuration.workers(),
            programme: self.programme.clone(),
        });
        let mut inits = Vec::new();
        solution.parcourir(&mut |s, parent| {
            inits.push(Evenement::Init {
                t: ctx.temps(),
                sol: s.id(),
                parent,
                regles: s.noms_regles().into_iter().map(str::to_string).collect(),
                molecules: s.molecules().cloned().collect(),
            });
        });
        self.diffuseur.diffuser_lot(&inits);
    }
}

/// Vrai si la phase de reaction d'une solution peut produire une reaction :
/// des regles, et des elements ou une regle sans reactif.
fn phase_utile(solution: &Solution) -> bool {
    !solution.regles.is_empty()
        && (!solution.elements.is_empty() || solution.regles.iter().any(|r| r.sans_reactif()))
}

/// Repartit `budget` workers entre la phase locale et `enfants` sous-solutions
/// actives ; retourne (budget de la phase, budget de chaque enfant).
fn repartir(budget: usize, enfants: usize, phase_utile: bool) -> (usize, usize) {
    let budget = budget.max(1);
    match (enfants, phase_utile) {
        (0, _) => (budget, 1),
        (k, false) => (1, (budget / k).max(1)),
        (k, true) => {
            let phase = (budget / (k + 1)).max(1);
            (phase, (budget.saturating_sub(phase) / k).max(1))
        }
    }
}

/// Pilote recursif d'une execution (interne).
struct Moteur<'c, 'a> {
    ctx: &'c ContexteExecution<'a>,
    pool: &'c Pool,
    graine: u64,
    workers: usize,
}

impl Moteur<'_, '_> {
    /// Stabilise une solution ; `budget` est le nombre de workers alloues a
    /// ce sous-arbre. Le budget est reparti de facon deterministe entre la
    /// phase locale et les sous-solutions actives : le decoupage en lots ne
    /// depend que de la structure, jamais du minutage.
    fn stabiliser(&self, solution: &mut Solution, budget: usize) -> Result<(), Arret> {
        let mut tour = 0u64;
        solution.inerte = false;
        loop {
            self.ctx.garde().verifier()?;
            self.recevoir(solution);
            // Les references sont construites avant l'extraction : elles
            // designent aussi les sous-solutions actives (en cours d'execution).
            let ports = if solution.regles.iter().any(|r| r.references > 0) {
                solution.ports()
            } else {
                Vec::new()
            };
            let mut actives = solution.extraire_sous_solutions_actives();
            let avait_actives = !actives.is_empty();
            let phase_utile = phase_utile(solution);
            let (budget_phase, budget_enfant) = repartir(budget, actives.len(), phase_utile);
            let stabiliser_actives = |actives: &mut Vec<Box<Solution>>| {
                self.pool
                    .pour_chaque_mut(actives, &|enfant: &mut Box<Solution>| {
                        self.stabiliser(enfant, budget_enfant)
                    })
            };
            // Une place de worker n'est consommee que si les deux branches ont
            // du travail : sinon la branche utile garde tout le pool.
            let (enfants, locales) = match (avait_actives, phase_utile) {
                (false, _) => (Vec::new(), self.phase(solution, tour, budget_phase, &ports)),
                (true, false) => (stabiliser_actives(&mut actives), Ok(0)),
                (true, true) => self.pool.joindre(
                    || stabiliser_actives(&mut actives),
                    || self.phase(solution, tour, budget_phase, &ports),
                ),
            };
            solution.reintegrer(actives);
            for resultat in enfants {
                resultat?;
            }
            let reactions = locales?;
            // Du courrier arrive apres la derniere passe : un tour de plus.
            if !avait_actives && reactions == 0 && !solution.boite.a_du_courrier() {
                solution.inerte = true;
                if self.ctx.diffuseur.est_actif() {
                    self.ctx.diffuseur.diffuser(&Evenement::Inertie {
                        t: self.ctx.temps(),
                        sol: solution.id(),
                    });
                }
                return Ok(());
            }
            tour += 1;
            self.ctx.compter_tour();
        }
    }

    /// Releve le courrier d'une solution (debut de tour).
    fn recevoir(&self, solution: &mut Solution) {
        let recues = solution.boite.relever();
        if recues.is_empty() {
            return;
        }
        if self.ctx.diffuseur.est_actif() {
            self.ctx.diffuseur.diffuser(&Evenement::Reception {
                t: self.ctx.temps(),
                sol: solution.id(),
                molecules: recues.iter().map(|m| m.id).collect(),
            });
        }
        self.ctx.ajouter_compteurs(&Compteurs {
            receptions: recues.len() as u64,
            ..Compteurs::default()
        });
        solution
            .elements
            .extend(recues.into_iter().map(Element::Molecule));
    }

    /// Releve le courrier de toutes les solutions (fin d'execution).
    fn relever_tout(&self, solution: &mut Solution) {
        self.recevoir(solution);
        for element in &mut solution.elements {
            if let Element::Solution(s) = element {
                self.relever_tout(s);
            }
        }
    }

    /// Fait reagir la solution (hors sous-solutions actives) jusqu'a son
    /// inertie locale ; les elements sont toujours restitues, meme en cas
    /// d'arret.
    fn phase(
        &self,
        solution: &mut Solution,
        tour: u64,
        budget: usize,
        ports: &[Port],
    ) -> Result<u64, Arret> {
        if !phase_utile(solution) {
            return Ok(0);
        }
        let regles = solution.regles.clone();
        let id = solution.id();
        let boite = Arc::clone(&solution.boite);
        let elements = std::mem::take(&mut solution.elements);
        let mut lots = self.partitionner(elements, id, tour, budget);
        let reacteur = Reacteur::new(self.ctx, id, &regles)
            .avec_ports(ports)
            .avec_boite(&boite);
        let mut total = 0;
        let mut premiere_erreur = None;
        let mut niveau = 0;
        loop {
            let resultats = self.pool.pour_chaque_mut(&mut lots, &|lot: &mut Lot| {
                reacteur.reagir(lot, Pool::indice_courant())
            });
            for resultat in resultats {
                match resultat {
                    Ok(n) => total += n,
                    Err(arret) => {
                        premiere_erreur.get_or_insert(arret);
                    }
                }
            }
            if premiere_erreur.is_some() || lots.len() <= 1 {
                break;
            }
            niveau += 1;
            lots = self.fusionner(lots, id, tour, niveau);
        }
        solution.elements = lots.into_iter().flat_map(Lot::en_elements).collect();
        // Les activations a usage unique qui ont reagi disparaissent.
        let mut rang = 0;
        solution.regles.retain(|_| {
            rang += 1;
            !reacteur.retiree(rang - 1)
        });
        match premiere_erreur {
            Some(arret) => Err(arret),
            None => Ok(total),
        }
    }

    fn graine_lot(&self, solution: IdSolution, tour: u64, niveau: u64, index: u64) -> u64 {
        Alea::deriver(self.graine, &[solution.0, tour, niveau, index])
    }

    fn nombre_lots(&self, taille: usize, budget: usize) -> usize {
        let budget = budget.min(self.workers);
        if budget <= 1 {
            1
        } else {
            (taille / TAILLE_MIN_LOT).clamp(1, budget)
        }
    }

    fn partitionner(
        &self,
        mut elements: Vec<crate::solution::Element>,
        id: IdSolution,
        tour: u64,
        budget: usize,
    ) -> Vec<Lot> {
        let nombre = self.nombre_lots(elements.len(), budget);
        if nombre <= 1 {
            return vec![Lot::new(elements, self.graine_lot(id, tour, 0, 0))];
        }
        if self.ctx.melanger {
            Alea::new(self.graine_lot(id, tour, 0, u64::MAX)).melanger(&mut elements);
        }
        // Distribution circulaire : equilibre la charge meme quand le source
        // est trie (intervalles) en strategie ordonnee.
        let taille = elements.len().div_ceil(nombre);
        let mut morceaux: Vec<Vec<_>> = (0..nombre).map(|_| Vec::with_capacity(taille)).collect();
        for (k, e) in elements.into_iter().enumerate() {
            morceaux[k % nombre].push(e);
        }
        morceaux
            .into_iter()
            .enumerate()
            .map(|(index, morceau)| Lot::new(morceau, self.graine_lot(id, tour, 0, index as u64)))
            .collect()
    }

    fn fusionner(&self, lots: Vec<Lot>, id: IdSolution, tour: u64, niveau: u64) -> Vec<Lot> {
        let total: usize = lots.iter().map(Lot::taille).sum();
        let cible = if total < 2 * TAILLE_MIN_LOT {
            1
        } else {
            lots.len().div_ceil(2)
        };
        let par_groupe = lots.len().div_ceil(cible);
        let mut fusionnes = Vec::with_capacity(cible);
        let mut iterateur = lots.into_iter();
        let mut index = 0;
        loop {
            let mut groupe = iterateur.by_ref().take(par_groupe);
            let Some(mut premier) = groupe.next() else {
                break;
            };
            for lot in groupe {
                premier.absorber(lot);
            }
            premier.graine = self.graine_lot(id, tour, niveau, index);
            index += 1;
            fusionnes.push(premier);
        }
        fusionnes
    }
}

#[cfg(test)]
mod tests {
    use super::repartir;

    #[test]
    fn repartition_du_budget() {
        assert_eq!(repartir(4, 0, true), (4, 1));
        assert_eq!(repartir(4, 8, false), (1, 1));
        assert_eq!(repartir(8, 2, false), (1, 4));
        assert_eq!(repartir(8, 3, true), (2, 2));
        assert_eq!(repartir(1, 3, true), (1, 1));
    }
}
