//! Pool fork-join borne a `N` workers, sur la bibliotheque standard.
//!
//! Interface volontairement calquee sur Rayon (`join`, `par_iter_mut`,
//! `current_thread_index`) : c'est le point d'integration de Rayon. Pour le
//! brancher, il suffit de reecrire ces trois fonctions avec
//! `rayon::join`, `par_iter_mut().map(f).collect()` et
//! `rayon::current_thread_index()` dans un `ThreadPool` a `N` threads.
//!
//! Fonctionnement : `joindre(a, b)` execute `b` sur un nouveau thread si une
//! place de worker est libre, sinon les deux sequentiellement sur le thread
//! courant. Aucune attente de place n'est bloquante : pas d'interblocage
//! possible, et au plus `N` threads travaillent simultanement. Avec `N = 1`,
//! tout s'execute sur le thread appelant (mode sequentiel exact, EF-S7).

use std::cell::Cell;
use std::sync::{Mutex, PoisonError};

/// Taille de pile des workers (recursion sur les solutions imbriquees).
const PILE_WORKER: usize = 16 * 1024 * 1024;

thread_local! {
    static INDICE_WORKER: Cell<usize> = const { Cell::new(0) };
}

/// Pool de workers.
pub struct Pool {
    places: Mutex<Vec<usize>>,
    workers: usize,
}

struct Liberation<'p> {
    pool: &'p Pool,
    place: usize,
}

impl Drop for Liberation<'_> {
    fn drop(&mut self) {
        self.pool.liberer(self.place);
    }
}

impl Pool {
    /// Pool de `workers` workers (au moins 1, le thread appelant compris).
    pub fn new(workers: usize) -> Self {
        let workers = workers.max(1);
        Self {
            places: Mutex::new((1..workers).rev().collect()),
            workers,
        }
    }

    /// Nombre de workers.
    pub fn workers(&self) -> usize {
        self.workers
    }

    /// Indice du worker courant (0 = thread appelant).
    pub fn indice_courant() -> usize {
        INDICE_WORKER.with(Cell::get)
    }

    /// Execute `f` en tant que worker 0.
    pub fn installer<R>(&self, f: impl FnOnce() -> R) -> R {
        INDICE_WORKER.with(|c| c.set(0));
        f()
    }

    fn reserver(&self) -> Option<usize> {
        self.places
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop()
    }

    fn liberer(&self, place: usize) {
        self.places
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(place);
    }

    /// Execute `a` et `b`, en parallele si possible (equivalent de `rayon::join`).
    pub fn joindre<A, B, RA, RB>(&self, a: A, b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send,
    {
        let Some(place) = self.reserver() else {
            let ra = a();
            return (ra, b());
        };
        let tache_b = Mutex::new(Some(b));
        std::thread::scope(|portee| {
            let lancement = std::thread::Builder::new()
                .name(format!("chem-worker-{place}"))
                .stack_size(PILE_WORKER)
                .spawn_scoped(portee, || {
                    let _liberation = Liberation { pool: self, place };
                    INDICE_WORKER.with(|c| c.set(place));
                    let b = tache_b
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take()
                        .expect("tache executee une seule fois");
                    b()
                });
            match lancement {
                Ok(poignee) => {
                    let ra = a();
                    let rb = match poignee.join() {
                        Ok(r) => r,
                        Err(panique) => std::panic::resume_unwind(panique),
                    };
                    (ra, rb)
                }
                Err(_) => {
                    // Creation de thread impossible : repli sequentiel.
                    self.liberer(place);
                    let b = tache_b
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take()
                        .expect("tache non executee");
                    let ra = a();
                    (ra, b())
                }
            }
        })
    }

    /// Applique `f` a chaque element, en parallele si possible ; resultats
    /// dans l'ordre (equivalent de `par_iter_mut().map(f).collect()`).
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn resultats_dans_l_ordre() {
        let pool = Pool::new(4);
        let mut v: Vec<u64> = (0..100).collect();
        let r = pool.pour_chaque_mut(&mut v, &|x: &mut u64| {
            *x += 1;
            *x * 2
        });
        assert_eq!(r, (1..=100).map(|x| x * 2).collect::<Vec<_>>());
        assert_eq!(v[99], 100);
    }

    #[test]
    fn parallelisme_borne() {
        let pool = Pool::new(3);
        let actifs = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let indices = Mutex::new(HashSet::new());
        let mut v = vec![0u8; 64];
        pool.pour_chaque_mut(&mut v, &|_| {
            let n = actifs.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(n, Ordering::SeqCst);
            indices.lock().unwrap().insert(Pool::indice_courant());
            std::thread::sleep(std::time::Duration::from_millis(2));
            actifs.fetch_sub(1, Ordering::SeqCst);
        });
        assert!(maximum.load(Ordering::SeqCst) <= 3);
        assert!(indices.lock().unwrap().iter().all(|&i| i < 3));
    }

    #[test]
    fn un_seul_worker_est_sequentiel() {
        let pool = Pool::new(1);
        let thread = std::thread::current().id();
        let mut v = vec![0u8; 16];
        pool.pour_chaque_mut(&mut v, &|_| assert_eq!(std::thread::current().id(), thread));
        let (a, b) = pool.joindre(|| 1, || 2);
        assert_eq!((a, b), (1, 2));
    }
}
