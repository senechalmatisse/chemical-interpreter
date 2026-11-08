//! Execution fork-join des taches de l'ordonnanceur.
//!
//! L'ordonnanceur ne depend que de l'interface [`Parallelisme`].
//! L'implementation par defaut est le [`Pool`] de la bibliotheque standard ;
//! le paquet `integrations` en fournit une avec Rayon. L'interface est
//! utilisable comme objet (`dyn`) : les taches y sont des closures `FnMut`
//! sans resultat, et [`Executeur`] permet d'y passer des closures
//! generiques qui retournent une valeur.
//!
//! Contrat d'une implementation :
//!
//! - `joindre(a, b)` execute `a` et `b` une fois chacune et ne rend la main
//!   qu'une fois les deux terminees ; une panique de l'une est propagee ;
//! - avec un seul worker, tout s'execute sur un meme thread, `a` avant `b`
//!   (le mode sequentiel est alors exactement reproductible) ;
//! - `indice_courant()` est dans `0..workers()` pendant `installer`.

use super::pool::Pool;

/// Pool de workers fork-join.
pub trait Parallelisme: Sync {
    /// Nombre de workers.
    fn workers(&self) -> usize;

    /// Indice du worker qui execute l'appel (dans `0..workers()`).
    fn indice_courant(&self) -> usize;

    /// Execute `tache` comme racine du calcul, sur les workers du pool.
    fn installer(&self, tache: &mut (dyn FnMut() + Send));

    /// Execute `a` et `b`, en parallele si un worker est libre, et attend
    /// les deux.
    fn joindre(&self, a: &mut (dyn FnMut() + Send), b: &mut (dyn FnMut() + Send));
}

/// Cree une implementation de [`Parallelisme`] pour chaque execution.
pub trait FabriqueParallelisme: Send + Sync {
    /// Nom de l'implementation (affiche par `chem --version`).
    fn nom(&self) -> &'static str;

    /// Pool de `workers` workers (au moins 1).
    fn creer(&self, workers: usize) -> Box<dyn Parallelisme>;
}

/// Pool de la bibliotheque standard ([`Pool`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct ParallelismeStandard;

impl FabriqueParallelisme for ParallelismeStandard {
    fn nom(&self) -> &'static str {
        "bibliotheque standard"
    }

    fn creer(&self, workers: usize) -> Box<dyn Parallelisme> {
        Box::new(Pool::new(workers))
    }
}

/// Acces generique a un [`Parallelisme`] : taches avec valeur de retour et
/// application parallele sur les elements d'une tranche.
#[derive(Clone, Copy)]
pub struct Executeur<'p> {
    parallelisme: &'p dyn Parallelisme,
}

impl<'p> Executeur<'p> {
    /// Enveloppe une implementation.
    pub fn new(parallelisme: &'p dyn Parallelisme) -> Self {
        Self { parallelisme }
    }

    /// Nombre de workers.
    pub fn workers(&self) -> usize {
        self.parallelisme.workers()
    }

    /// Indice du worker courant.
    pub fn indice_courant(&self) -> usize {
        self.parallelisme.indice_courant()
    }

    /// Execute `tache` sur le pool et retourne son resultat.
    pub fn installer<R: Send>(&self, tache: impl FnOnce() -> R + Send) -> R {
        let mut tache = Some(tache);
        let mut resultat = None;
        self.parallelisme.installer(&mut || {
            if let Some(t) = tache.take() {
                resultat = Some(t());
            }
        });
        resultat.expect("la tache installee a ete executee")
    }

    /// Execute `a` et `b`, en parallele si possible, et retourne leurs
    /// resultats.
    pub fn joindre<A, B, RA, RB>(&self, a: A, b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send,
    {
        let (mut a, mut b) = (Some(a), Some(b));
        let (mut ra, mut rb) = (None, None);
        self.parallelisme.joindre(
            &mut || {
                if let Some(f) = a.take() {
                    ra = Some(f());
                }
            },
            &mut || {
                if let Some(f) = b.take() {
                    rb = Some(f());
                }
            },
        );
        (
            ra.expect("la premiere tache a ete executee"),
            rb.expect("la seconde tache a ete executee"),
        )
    }

    /// Applique `f` a chaque element, en parallele si possible ; resultats
    /// dans l'ordre des elements.
    pub fn pour_chaque_mut<T, R, F>(&self, elements: &mut [T], f: &F) -> Vec<R>
    where
        T: Send,
        R: Send,
        F: Fn(&mut T) -> R + Sync,
    {
        match elements.len() {
            0 => Vec::new(),
            1 => vec![f(&mut elements[0])],
            n => {
                let (gauche, droite) = elements.split_at_mut(n / 2);
                let (mut rg, rd) = self.joindre(
                    || self.pour_chaque_mut(gauche, f),
                    || self.pour_chaque_mut(droite, f),
                );
                rg.extend(rd);
                rg
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn resultats_dans_l_ordre() {
        let pool = ParallelismeStandard.creer(4);
        let executeur = Executeur::new(&*pool);
        let mut v: Vec<u64> = (0..100).collect();
        let r = executeur.installer(|| {
            executeur.pour_chaque_mut(&mut v, &|x: &mut u64| {
                *x += 1;
                *x * 2
            })
        });
        assert_eq!(r, (1..=100).map(|x| x * 2).collect::<Vec<_>>());
        assert_eq!(v[99], 100);
        assert_eq!(executeur.joindre(|| "a", || 2), ("a", 2));
    }

    #[test]
    fn parallelisme_borne() {
        let pool = ParallelismeStandard.creer(3);
        let executeur = Executeur::new(&*pool);
        let actifs = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let indices = Mutex::new(HashSet::new());
        let mut v = vec![0u8; 64];
        executeur.installer(|| {
            executeur.pour_chaque_mut(&mut v, &|_| {
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
        let pool = ParallelismeStandard.creer(1);
        let executeur = Executeur::new(&*pool);
        let thread = std::thread::current().id();
        let ordre = Mutex::new(Vec::new());
        executeur.installer(|| {
            let mut v = vec![0u8; 16];
            executeur.pour_chaque_mut(&mut v, &|_| {
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
        let pool = ParallelismeStandard.creer(2);
        let executeur = Executeur::new(&*pool);
        let resultat = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            executeur.joindre(|| 1, || panic!("tache en echec"))
        }));
        assert!(resultat.is_err());
    }
}
