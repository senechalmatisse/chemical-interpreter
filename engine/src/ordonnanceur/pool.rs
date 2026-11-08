//! Pool fork-join borne a `N` workers, sur la bibliotheque standard.
//!
//! Implementation par defaut de [`Parallelisme`] (le paquet `integrations`
//! en fournit une avec Rayon).
//!
//! Fonctionnement : `joindre(a, b)` execute `b` sur un nouveau thread si une
//! place de worker est libre, sinon les deux sequentiellement sur le thread
//! courant. Aucune attente de place n'est bloquante : pas d'interblocage
//! possible, et au plus `N` threads travaillent simultanement. Avec `N = 1`,
//! tout s'execute sur le thread appelant (mode sequentiel exact).

use super::parallelisme::Parallelisme;
use std::cell::Cell;
use std::sync::{Mutex, PoisonError};

/// Taille de pile des workers (recursion sur les solutions imbriquees).
pub const PILE_WORKER: usize = 16 * 1024 * 1024;

thread_local! {
    static INDICE_WORKER: Cell<usize> = const { Cell::new(0) };
}

/// Pool de workers de la bibliotheque standard.
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
}

impl Parallelisme for Pool {
    fn workers(&self) -> usize {
        self.workers
    }

    fn indice_courant(&self) -> usize {
        INDICE_WORKER.with(Cell::get)
    }

    /// Le thread appelant est le worker 0.
    fn installer(&self, tache: &mut (dyn FnMut() + Send)) {
        INDICE_WORKER.with(|c| c.set(0));
        tache();
    }

    fn joindre(&self, a: &mut (dyn FnMut() + Send), b: &mut (dyn FnMut() + Send)) {
        let Some(place) = self.reserver() else {
            a();
            b();
            return;
        };
        // `b` passe au thread s'il est cree, sinon il est repris ici.
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
                        .take();
                    if let Some(b) = b {
                        b();
                    }
                });
            match lancement {
                Ok(poignee) => {
                    a();
                    if let Err(panique) = poignee.join() {
                        std::panic::resume_unwind(panique);
                    }
                }
                Err(_) => {
                    // Creation de thread impossible : repli sequentiel.
                    self.liberer(place);
                    let b = tache_b
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                    a();
                    if let Some(b) = b {
                        b();
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_rendues_apres_chaque_jonction() {
        let pool = Pool::new(2);
        for _ in 0..3 {
            let (mut a, mut b) = (false, false);
            pool.joindre(&mut || a = true, &mut || b = true);
            assert!(a && b);
        }
        assert_eq!(pool.reserver(), Some(1));
    }
}
