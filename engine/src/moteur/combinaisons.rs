//! Iterateurs de combinaisons et de permutations (regles d'arite >= 3).

/// Combinaisons de `k` indices parmi `0..n`, en ordre lexicographique.
pub struct Combinaisons {
    n: usize,
    courant: Vec<usize>,
    premier: bool,
    fini: bool,
}

impl Combinaisons {
    /// Iterateur des combinaisons de `k` parmi `n`.
    pub fn new(n: usize, k: usize) -> Self {
        Self {
            n,
            courant: (0..k).collect(),
            premier: true,
            fini: k > n,
        }
    }
}

impl Iterator for Combinaisons {
    type Item = Vec<usize>;

    fn next(&mut self) -> Option<Vec<usize>> {
        if self.fini {
            return None;
        }
        if self.premier {
            self.premier = false;
            return Some(self.courant.clone());
        }
        let k = self.courant.len();
        let mut i = k;
        while i > 0 {
            i -= 1;
            if self.courant[i] < self.n - k + i {
                self.courant[i] += 1;
                for j in i + 1..k {
                    self.courant[j] = self.courant[j - 1] + 1;
                }
                return Some(self.courant.clone());
            }
        }
        self.fini = true;
        None
    }
}

/// Permutations d'un vecteur (algorithme de Heap, iteratif).
pub struct Permutations<T: Clone> {
    elements: Vec<T>,
    compteurs: Vec<usize>,
    i: usize,
    premier: bool,
}

impl<T: Clone> Permutations<T> {
    /// Iterateur de toutes les permutations de `elements`.
    pub fn new(elements: Vec<T>) -> Self {
        let n = elements.len();
        Self {
            elements,
            compteurs: vec![0; n],
            i: 1,
            premier: true,
        }
    }
}

impl<T: Clone> Iterator for Permutations<T> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Vec<T>> {
        if self.premier {
            self.premier = false;
            return Some(self.elements.clone());
        }
        let n = self.elements.len();
        while self.i < n {
            if self.compteurs[self.i] < self.i {
                if self.i.is_multiple_of(2) {
                    self.elements.swap(0, self.i);
                } else {
                    self.elements.swap(self.compteurs[self.i], self.i);
                }
                self.compteurs[self.i] += 1;
                self.i = 1;
                return Some(self.elements.clone());
            }
            self.compteurs[self.i] = 0;
            self.i += 1;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn combinaisons() {
        let toutes: Vec<_> = Combinaisons::new(4, 2).collect();
        assert_eq!(
            toutes,
            vec![
                vec![0, 1],
                vec![0, 2],
                vec![0, 3],
                vec![1, 2],
                vec![1, 3],
                vec![2, 3]
            ]
        );
        assert_eq!(Combinaisons::new(5, 3).count(), 10);
        assert_eq!(Combinaisons::new(2, 3).count(), 0);
        assert_eq!(Combinaisons::new(3, 0).count(), 1);
    }

    #[test]
    fn permutations() {
        let toutes: HashSet<Vec<u8>> = Permutations::new(vec![1, 2, 3, 4]).collect();
        assert_eq!(toutes.len(), 24);
        assert_eq!(Permutations::new(vec![7]).count(), 1);
    }
}
