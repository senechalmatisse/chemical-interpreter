//! Contexte partage par les workers : garde-fous, identifiants, trace,
//! compteurs. Toutes les donnees partagees sont atomiques ou protegees par
//! des verrous de courte duree ; aucun verrou n'est tenu pendant une
//! recherche de reactifs.

use crate::annulation::JetonAnnulation;
use crate::diagnostic::Diagnostic;
use crate::evaluateur::ErreurEvaluation;
use crate::solution::{GenerateurIds, IdSolution};
use crate::trace::Diffuseur;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Erreur d'execution (division par zero, type...) dans une regle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErreurExecution {
    /// Regle en cause.
    pub regle: String,
    /// Solution ou la reaction etait tentee.
    pub solution: IdSolution,
    /// Reactifs (valeurs affichees).
    pub reactifs: Vec<String>,
    /// Erreur d'evaluation.
    pub erreur: ErreurEvaluation,
}

impl ErreurExecution {
    /// Diagnostic localise dans le source.
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::erreur(
            self.erreur.code(),
            format!(
                "erreur d'execution dans la regle '{}' : {}",
                self.regle, self.erreur
            ),
        )
        .etiquette(self.erreur.span, "operation fautive")
        .aide(format!(
            "reactifs : {} (solution {})",
            self.reactifs.join(", "),
            self.solution
        ))
    }
}

/// Cause d'un arret avant l'inertie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arret {
    /// `--max-etapes` atteint.
    MaxEtapes(u64),
    /// `--delai-max` depasse.
    Delai(Duration),
    /// `--max-molecules` depasse.
    MaxMolecules(u64),
    /// Arret demande (pas-a-pas, jeton d'annulation).
    Interruption,
    /// Erreur d'execution.
    Erreur(ErreurExecution),
}

impl Arret {
    /// Motif court, repris dans l'evenement `fin`.
    pub fn motif(&self) -> &'static str {
        match self {
            Arret::MaxEtapes(_) => "max-etapes",
            Arret::Delai(_) => "delai",
            Arret::MaxMolecules(_) => "max-molecules",
            Arret::Interruption => "interruption",
            Arret::Erreur(_) => "erreur",
        }
    }

    /// Vrai pour un garde-fou (par opposition a une erreur).
    pub fn est_garde_fou(&self) -> bool {
        !matches!(self, Arret::Erreur(_))
    }
}

impl fmt::Display for Arret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arret::MaxEtapes(n) => {
                write!(f, "nombre maximal d'etapes atteint ({n}) avant l'inertie")
            }
            Arret::Delai(d) => write!(
                f,
                "delai maximal depasse ({:.3} s) avant l'inertie",
                d.as_secs_f64()
            ),
            Arret::MaxMolecules(n) => write!(f, "nombre maximal de molecules depasse ({n})"),
            Arret::Interruption => f.write_str("execution interrompue a la demande"),
            Arret::Erreur(e) => write!(
                f,
                "erreur d'execution dans la regle '{}' : {}",
                e.regle, e.erreur
            ),
        }
    }
}

/// Garde-fous partages : budget d'etapes, echeance, taille.
pub struct GardeFous {
    budget: Option<AtomicU64>,
    limite_etapes: u64,
    echeance: Option<(Instant, Duration)>,
    max_molecules: Option<u64>,
    molecules: AtomicI64,
    arret: AtomicBool,
    cause: Mutex<Option<Arret>>,
    annulation: Option<JetonAnnulation>,
}

impl GardeFous {
    /// Garde-fous pour une execution.
    pub fn new(
        max_etapes: Option<u64>,
        delai_max: Option<Duration>,
        max_molecules: Option<u64>,
        molecules_initiales: u64,
        annulation: Option<JetonAnnulation>,
        debut: Instant,
    ) -> Self {
        Self {
            budget: max_etapes.map(AtomicU64::new),
            limite_etapes: max_etapes.unwrap_or(u64::MAX),
            echeance: delai_max.map(|d| (debut + d, d)),
            max_molecules,
            molecules: AtomicI64::new(molecules_initiales as i64),
            arret: AtomicBool::new(false),
            cause: Mutex::new(None),
            annulation,
        }
    }

    /// Preleve jusqu'a `voulu` jetons d'etape ; 0 si le budget est epuise.
    pub fn prendre_jetons(&self, voulu: u64) -> u64 {
        let Some(budget) = &self.budget else {
            return voulu;
        };
        let mut actuel = budget.load(Ordering::Relaxed);
        loop {
            if actuel == 0 {
                return 0;
            }
            let pris = actuel.min(voulu);
            match budget.compare_exchange_weak(
                actuel,
                actuel - pris,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return pris,
                Err(v) => actuel = v,
            }
        }
    }

    /// Restitue des jetons inutilises.
    pub fn rendre_jetons(&self, n: u64) {
        if let Some(budget) = &self.budget
            && n > 0
        {
            budget.fetch_add(n, Ordering::AcqRel);
        }
    }

    /// Limite d'etapes configuree.
    pub fn limite_etapes(&self) -> u64 {
        self.limite_etapes
    }

    /// Controle rapide, sans lecture d'horloge (avant chaque reaction) :
    /// arret signale par un autre worker ou annulation demandee.
    #[inline]
    pub fn controle_rapide(&self) -> Result<(), Arret> {
        if self.arret.load(Ordering::Acquire) {
            return Err(Arret::Interruption);
        }
        if let Some(jeton) = &self.annulation
            && jeton.est_demande()
        {
            return Err(self.signaler(Arret::Interruption));
        }
        Ok(())
    }

    /// Point de controle : arret demande, echeance, annulation.
    pub fn verifier(&self) -> Result<(), Arret> {
        if self.arret.load(Ordering::Acquire) {
            return Err(Arret::Interruption);
        }
        if let Some(jeton) = &self.annulation
            && jeton.est_demande()
        {
            return Err(self.signaler(Arret::Interruption));
        }
        if let Some((fin, duree)) = self.echeance
            && Instant::now() >= fin
        {
            return Err(self.signaler(Arret::Delai(duree)));
        }
        Ok(())
    }

    /// Enregistre la premiere cause d'arret et previent tous les workers.
    pub fn signaler(&self, arret: Arret) -> Arret {
        let mut cause = self.cause.lock().unwrap_or_else(PoisonError::into_inner);
        if cause.is_none() {
            *cause = Some(arret.clone());
        }
        self.arret.store(true, Ordering::Release);
        arret
    }

    /// Met a jour le nombre approximatif de molecules et controle la limite.
    pub fn ajuster_molecules(&self, delta: i64) -> Result<(), Arret> {
        if delta == 0 {
            return Ok(());
        }
        let total = self.molecules.fetch_add(delta, Ordering::AcqRel) + delta;
        match self.max_molecules {
            Some(max) if total > max as i64 => Err(self.signaler(Arret::MaxMolecules(max))),
            _ => Ok(()),
        }
    }

    /// Premiere cause d'arret enregistree.
    pub fn cause(&self) -> Option<Arret> {
        self.cause
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Compteurs agreges de l'execution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compteurs {
    /// Reactions realisees.
    pub reactions: u64,
    /// Reactions par regle.
    pub par_regle: BTreeMap<String, u64>,
    /// Reactions par worker.
    pub par_worker: BTreeMap<usize, u64>,
    /// Dissolutions.
    pub dissolutions: u64,
    /// Injections.
    pub injections: u64,
    /// Sous-solutions detruites.
    pub destructions: u64,
    /// Passes de recherche exhaustive.
    pub passes: u64,
    /// Molecules envoyees par injection asynchrone (EF-D6).
    pub envois: u64,
    /// Molecules recues (courrier releve).
    pub receptions: u64,
    /// Activations de regles a usage unique consommees (EF-D5).
    pub regles_retirees: u64,
}

impl Compteurs {
    /// Ajoute d'autres compteurs.
    pub fn fusionner(&mut self, autre: &Compteurs) {
        self.reactions += autre.reactions;
        for (k, v) in &autre.par_regle {
            *self.par_regle.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &autre.par_worker {
            *self.par_worker.entry(*k).or_default() += v;
        }
        self.dissolutions += autre.dissolutions;
        self.injections += autre.injections;
        self.destructions += autre.destructions;
        self.passes += autre.passes;
        self.envois += autre.envois;
        self.receptions += autre.receptions;
        self.regles_retirees += autre.regles_retirees;
    }
}

/// Contexte d'execution partage (lecture concurrente).
pub struct ContexteExecution<'a> {
    pub(crate) garde: GardeFous,
    pub(crate) ids: &'a GenerateurIds,
    pub(crate) diffuseur: &'a Diffuseur,
    pub(crate) melanger: bool,
    horloge: Instant,
    numero: AtomicU64,
    tours: AtomicU64,
    compteurs: Mutex<Compteurs>,
}

impl<'a> ContexteExecution<'a> {
    /// Nouveau contexte.
    pub fn new(
        garde: GardeFous,
        ids: &'a GenerateurIds,
        diffuseur: &'a Diffuseur,
        melanger: bool,
        horloge: Instant,
    ) -> Self {
        Self {
            garde,
            ids,
            diffuseur,
            melanger,
            horloge,
            numero: AtomicU64::new(0),
            tours: AtomicU64::new(0),
            compteurs: Mutex::new(Compteurs::default()),
        }
    }

    /// Secondes ecoulees depuis le debut.
    pub fn temps(&self) -> f64 {
        self.horloge.elapsed().as_secs_f64()
    }

    /// Numero de la prochaine reaction tracee.
    pub fn numero_suivant(&self) -> u64 {
        self.numero.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Compte un tour supplementaire d'une solution.
    pub fn compter_tour(&self) {
        self.tours.fetch_add(1, Ordering::Relaxed);
    }

    /// Nombre de tours realises.
    pub fn tours(&self) -> u64 {
        self.tours.load(Ordering::Relaxed)
    }

    /// Garde-fous.
    pub fn garde(&self) -> &GardeFous {
        &self.garde
    }

    /// Ajoute des compteurs locaux (fin de lot).
    pub fn ajouter_compteurs(&self, locaux: &Compteurs) {
        self.compteurs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .fusionner(locaux);
    }

    /// Compteurs agreges.
    pub fn compteurs(&self) -> Compteurs {
        self.compteurs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_d_etapes_exact() {
        let g = GardeFous::new(Some(10), None, None, 0, None, Instant::now());
        assert_eq!(g.prendre_jetons(4), 4);
        assert_eq!(g.prendre_jetons(8), 6);
        assert_eq!(g.prendre_jetons(1), 0);
        g.rendre_jetons(2);
        assert_eq!(g.prendre_jetons(5), 2);
    }

    #[test]
    fn premiere_cause_conservee() {
        let g = GardeFous::new(None, None, Some(3), 2, None, Instant::now());
        assert!(g.verifier().is_ok());
        assert_eq!(g.ajuster_molecules(2), Err(Arret::MaxMolecules(3)));
        g.signaler(Arret::Interruption);
        assert_eq!(g.cause(), Some(Arret::MaxMolecules(3)));
        assert_eq!(g.verifier(), Err(Arret::Interruption));
    }

    #[test]
    fn echeance_et_annulation() {
        let jeton = JetonAnnulation::new();
        let g = GardeFous::new(
            None,
            Some(Duration::ZERO),
            None,
            0,
            Some(jeton.clone()),
            Instant::now(),
        );
        assert!(matches!(g.verifier(), Err(Arret::Delai(_))));
        let g = GardeFous::new(None, None, None, 0, Some(jeton.clone()), Instant::now());
        jeton.demander();
        assert_eq!(g.verifier(), Err(Arret::Interruption));
    }
}
