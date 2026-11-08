//! Generateur pseudo-aleatoire deterministe (SplitMix64).
//!
//! Une meme graine produit la meme suite sur toutes les plateformes : c'est
//! ce qui rend les executions aleatoires rejouables (`--seed`).

use std::time::{SystemTime, UNIX_EPOCH};

/// Generateur SplitMix64.
#[derive(Debug, Clone)]
pub struct Alea {
    etat: u64,
}

impl Alea {
    /// Generateur initialise par une graine.
    pub fn new(graine: u64) -> Self {
        Self { etat: graine }
    }

    /// Generateur place directement au tirage `rang` de la suite de
    /// `graine` : son premier tirage est le tirage `rang` (compte a partir
    /// de 0) de `Alea::new(graine)`. L'etat de SplitMix64 avance d'une
    /// constante a chaque tirage : des taches paralleles peuvent ainsi
    /// tirer chacune sa part d'une meme suite, sans dependre les unes des
    /// autres.
    pub fn au_rang(graine: u64, rang: u64) -> Self {
        Self {
            etat: graine.wrapping_add(rang.wrapping_mul(0x9E37_79B9_7F4A_7C15)),
        }
    }

    /// Entier pseudo-aleatoire suivant.
    pub fn suivant(&mut self) -> u64 {
        self.etat = self.etat.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.etat;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Indice uniforme dans `0..n` (`n > 0`), methode multiplicative de Lemire.
    pub fn indice(&mut self, n: usize) -> usize {
        debug_assert!(n > 0);
        ((u128::from(self.suivant()) * n as u128) >> 64) as usize
    }

    /// Entier dans l'intervalle `[min, max]`.
    pub fn entre(&mut self, min: i64, max: i64) -> i64 {
        debug_assert!(min <= max);
        let etendue = (max as i128 - min as i128 + 1) as u128;
        (min as i128 + ((u128::from(self.suivant()) * etendue) >> 64) as i128) as i64
    }

    /// Booleen de probabilite `p`.
    pub fn probabilite(&mut self, p: f64) -> bool {
        (self.suivant() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }

    /// Melange de Fisher-Yates.
    pub fn melanger<T>(&mut self, elements: &mut [T]) {
        for i in (1..elements.len()).rev() {
            let j = self.indice(i + 1);
            elements.swap(i, j);
        }
    }

    /// Derive une graine independante a partir d'une graine et de composantes
    /// (solution, tour, niveau, partition...).
    pub fn deriver(graine: u64, composantes: &[u64]) -> u64 {
        let mut a = Alea::new(graine);
        let mut h = a.suivant();
        for &c in composantes {
            a = Alea::new(h ^ c.wrapping_mul(0xD6E8_FEB8_6659_FD93));
            h = a.suivant();
        }
        h
    }

    /// Graine tiree de l'horloge (quand l'utilisateur n'en donne pas).
    pub fn graine_horloge() -> u64 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Alea::new(nanos ^ u64::from(std::process::id()).rotate_left(32)).suivant()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministe_et_borne() {
        let mut a = Alea::new(42);
        let mut b = Alea::new(42);
        for _ in 0..100 {
            assert_eq!(a.suivant(), b.suivant());
        }
        for n in 1..50 {
            assert!(a.indice(n) < n);
        }
        for _ in 0..1000 {
            let x = a.entre(-3, 3);
            assert!((-3..=3).contains(&x));
        }
        assert_eq!(a.entre(i64::MIN, i64::MIN), i64::MIN);
    }

    #[test]
    fn melange_est_une_permutation() {
        let mut v: Vec<u32> = (0..100).collect();
        Alea::new(7).melanger(&mut v);
        assert_ne!(v, (0..100).collect::<Vec<_>>());
        v.sort_unstable();
        assert_eq!(v, (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn acces_direct_a_un_rang() {
        let mut suite = Alea::new(42);
        for rang in 0..100 {
            assert_eq!(Alea::au_rang(42, rang).suivant(), suite.suivant());
        }
        assert_eq!(Alea::au_rang(7, 0).suivant(), Alea::new(7).suivant());
    }

    #[test]
    fn derivation_distincte() {
        assert_ne!(Alea::deriver(1, &[0, 1]), Alea::deriver(1, &[1, 0]));
        assert_eq!(Alea::deriver(1, &[2, 3]), Alea::deriver(1, &[2, 3]));
    }
}
