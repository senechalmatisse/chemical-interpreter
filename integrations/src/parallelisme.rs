//! Pool de workers avec Rayon.
//!
//! Point d'integration : `chem::ordonnanceur::FabriqueParallelisme`.
//!
//! Le pool de la bibliotheque (`chem::ordonnanceur::Pool`) demarre un
//! thread pour chaque branche executee en parallele. Celui-ci s'appuie sur
//! un `rayon::ThreadPool` cree pour l'execution : les threads sont demarres
//! une seule fois et le vol de taches equilibre la charge entre les
//! sous-solutions.
//!
//! Le contrat de l'interface (documente dans `chem::ordonnanceur`) est
//! respecte :
//!
//! - `joindre` est `ThreadPool::join`, qui attend les deux taches et
//!   propage leurs paniques ;
//! - avec un seul worker, Rayon execute `a` puis `b` sur l'unique thread du
//!   pool : le mode sequentiel reste reproductible ;
//! - `indice_courant` est l'indice du thread dans le pool Rayon.
//!
//! Les threads ont la pile des workers du pool standard
//! (`chem::ordonnanceur::PILE_WORKER`) : la profondeur d'imbrication des
//! solutions supportee est la meme. Si le systeme refuse la creation des
//! threads, la fabrique rend le pool standard.

use chem::ordonnanceur::{FabriqueParallelisme, PILE_WORKER, Parallelisme, Pool};

/// Fabrique un pool Rayon pour chaque execution.
#[derive(Debug, Default, Clone, Copy)]
pub struct FabriqueRayon;

impl FabriqueParallelisme for FabriqueRayon {
    fn nom(&self) -> &'static str {
        "rayon"
    }

    fn creer(&self, workers: usize) -> Box<dyn Parallelisme> {
        match ParallelismeRayon::new(workers) {
            Ok(pool) => Box::new(pool),
            Err(_) => Box::new(Pool::new(workers)),
        }
    }
}

/// Pool Rayon dedie a une execution.
#[derive(Debug)]
pub struct ParallelismeRayon {
    pool: rayon::ThreadPool,
}

impl ParallelismeRayon {
    /// Pool de `workers` threads (au moins un).
    pub fn new(workers: usize) -> Result<Self, rayon::ThreadPoolBuildError> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers.max(1))
            .stack_size(PILE_WORKER)
            .thread_name(|indice| format!("chem-rayon-{indice}"))
            .build()?;
        Ok(Self { pool })
    }
}

impl Parallelisme for ParallelismeRayon {
    fn workers(&self) -> usize {
        self.pool.current_num_threads()
    }

    fn indice_courant(&self) -> usize {
        self.pool.current_thread_index().unwrap_or(0)
    }

    fn installer(&self, tache: &mut (dyn FnMut() + Send)) {
        self.pool.install(tache);
    }

    fn joindre(&self, a: &mut (dyn FnMut() + Send), b: &mut (dyn FnMut() + Send)) {
        self.pool.join(a, b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem::ordonnanceur::Executeur;
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn resultats_dans_l_ordre() {
        let pool = FabriqueRayon.creer(4);
        let executeur = Executeur::new(&*pool);
        let mut valeurs: Vec<u64> = (0..100).collect();
        let resultats = executeur.installer(|| {
            executeur.pour_chaque_mut(&mut valeurs, &|x: &mut u64| {
                *x += 1;
                *x * 2
            })
        });
        assert_eq!(resultats, (1..=100).map(|x| x * 2).collect::<Vec<_>>());
        assert_eq!(valeurs[99], 100);
        assert_eq!(executeur.joindre(|| "a", || 2), ("a", 2));
    }

    #[test]
    fn parallelisme_borne_et_indices_valides() {
        let pool = FabriqueRayon.creer(3);
        let executeur = Executeur::new(&*pool);
        let actifs = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let indices = Mutex::new(HashSet::new());
        let mut elements = vec![0u8; 64];
        executeur.installer(|| {
            executeur.pour_chaque_mut(&mut elements, &|_| {
                let n = actifs.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(n, Ordering::SeqCst);
                indices.lock().unwrap().insert(executeur.indice_courant());
                std::thread::sleep(std::time::Duration::from_millis(2));
                actifs.fetch_sub(1, Ordering::SeqCst);
            })
        });
        assert!(maximum.load(Ordering::SeqCst) <= 3);
        assert!(indices.lock().unwrap().iter().all(|&i| i < 3));
        assert_eq!(executeur.workers(), 3);
    }

    #[test]
    fn un_seul_worker_est_sequentiel() {
        let pool = FabriqueRayon.creer(1);
        let executeur = Executeur::new(&*pool);
        let ordre = Mutex::new(Vec::new());
        executeur.installer(|| {
            let thread = std::thread::current().id();
            let mut elements = vec![0u8; 16];
            executeur.pour_chaque_mut(&mut elements, &|_| {
                assert_eq!(std::thread::current().id(), thread);
            });
            executeur.joindre(
                || ordre.lock().unwrap().push('a'),
                || ordre.lock().unwrap().push('b'),
            );
        });
        assert_eq!(*ordre.lock().unwrap(), vec!['a', 'b']);
    }

    #[test]
    fn panique_propagee() {
        let pool = FabriqueRayon.creer(2);
        let executeur = Executeur::new(&*pool);
        let resultat = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            executeur.installer(|| executeur.joindre(|| 1, || panic!("tache en echec")))
        }));
        assert!(resultat.is_err());
    }
}
