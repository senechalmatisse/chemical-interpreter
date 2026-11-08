//! Reconstruction de l'etat des solutions a partir des evenements.
//!
//! Le modele verifie la coherence de chaque evenement : une molecule absente
//! (consommee deux fois), un identifiant reutilise, une sous-solution
//! inconnue, une regle qui n'est pas (ou plus) active ou une molecule recue
//! sans avoir ete envoyee sont des erreurs. Il sert a la trace console, au
//! rejeu et aux tests de correction du moteur parallele.

use super::Evenement;
use crate::evaluateur::Valeur;
use crate::solution::{IdMolecule, IdSolution, formater_canonique};
use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Debug, Clone, Default)]
struct Noeud {
    parent: Option<IdSolution>,
    regles: Vec<String>,
    molecules: BTreeMap<IdMolecule, Valeur>,
    /// Molecules envoyees a cette solution et pas encore recues.
    boite: BTreeMap<IdMolecule, Valeur>,
    enfants: BTreeSet<IdSolution>,
    inerte: bool,
}

/// Resume de l'evenement `fin`.
#[derive(Debug, Clone, PartialEq)]
pub struct FinTrace {
    /// Nombre de reactions annonce.
    pub reactions: u64,
    /// Inertie annoncee.
    pub inerte: bool,
    /// Motif de fin.
    pub motif: String,
}

/// Etat reconstruit d'une execution.
#[derive(Debug, Clone, Default)]
pub struct ModeleTrace {
    solutions: BTreeMap<IdSolution, Noeud>,
    racine: Option<IdSolution>,
    vus: HashSet<IdMolecule>,
    /// Reactions appliquees.
    pub reactions: u64,
    /// Evenement de fin, s'il a ete recu.
    pub fin: Option<FinTrace>,
    /// Graine annoncee par l'evenement `debut`.
    pub graine: Option<u64>,
}

impl ModeleTrace {
    /// Modele vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Solution principale.
    pub fn racine(&self) -> Option<IdSolution> {
        self.racine
    }

    /// Vrai si la solution existe.
    pub fn contient(&self, sol: IdSolution) -> bool {
        self.solutions.contains_key(&sol)
    }

    /// Valeur d'une molecule d'une solution.
    pub fn valeur(&self, sol: IdSolution, molecule: IdMolecule) -> Option<&Valeur> {
        self.solutions.get(&sol)?.molecules.get(&molecule)
    }

    /// Parent d'une solution.
    pub fn parent(&self, sol: IdSolution) -> Option<IdSolution> {
        self.solutions.get(&sol)?.parent
    }

    /// Inertie d'une solution : indicateur d'inertie et aucun courrier en
    /// attente (une sous-solution qui a du courrier ne peut pas etre liee).
    pub fn est_inerte(&self, sol: IdSolution) -> bool {
        self.solutions
            .get(&sol)
            .is_some_and(|n| n.inerte && n.boite.is_empty())
    }

    /// Nombre total de molecules (hors molecules en transit).
    pub fn nombre_molecules(&self) -> usize {
        self.solutions.values().map(|n| n.molecules.len()).sum()
    }

    /// Nombre total de molecules envoyees et pas encore recues.
    pub fn nombre_en_transit(&self) -> usize {
        self.solutions.values().map(|n| n.boite.len()).sum()
    }

    /// Molecules en transit vers une solution.
    pub fn en_transit(&self, sol: IdSolution) -> Vec<&Valeur> {
        self.solutions
            .get(&sol)
            .map(|n| n.boite.values().collect())
            .unwrap_or_default()
    }

    /// Valeur d'une molecule en transit vers une solution.
    pub fn valeur_en_transit(&self, sol: IdSolution, molecule: IdMolecule) -> Option<&Valeur> {
        self.solutions.get(&sol)?.boite.get(&molecule)
    }

    /// Solutions ayant du courrier en attente, dans l'ordre des identifiants.
    pub fn destinataires(&self) -> Vec<IdSolution> {
        self.solutions
            .iter()
            .filter(|(_, n)| !n.boite.is_empty())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Regles actives d'une solution.
    pub fn regles(&self, sol: IdSolution) -> Option<&[String]> {
        self.solutions.get(&sol).map(|n| n.regles.as_slice())
    }

    /// Nombre de sous-solutions directes d'une solution.
    pub fn nombre_enfants(&self, sol: IdSolution) -> usize {
        self.solutions.get(&sol).map_or(0, |n| n.enfants.len())
    }

    /// Texte canonique d'une solution (meme format que le moteur).
    pub fn solution_canonique(&self, sol: IdSolution) -> Option<String> {
        let noeud = self.solutions.get(&sol)?;
        let sous = noeud
            .enfants
            .iter()
            .filter_map(|e| self.solution_canonique(*e))
            .collect();
        let regles: Vec<&str> = noeud.regles.iter().map(String::as_str).collect();
        Some(formater_canonique(
            noeud.molecules.values().collect(),
            sous,
            &regles,
        ))
    }

    /// Texte canonique de la solution principale.
    pub fn racine_canonique(&self) -> Option<String> {
        self.solution_canonique(self.racine?)
    }

    fn noeud(&mut self, sol: IdSolution) -> Result<&mut Noeud, String> {
        self.solutions
            .get_mut(&sol)
            .ok_or_else(|| format!("solution {sol} inconnue"))
    }

    fn enfant_de(&self, sol: IdSolution, cible: IdSolution) -> Result<(), String> {
        match self.solutions.get(&cible) {
            Some(n) if n.parent == Some(sol) => Ok(()),
            Some(_) => Err(format!("{cible} n'est pas une sous-solution de {sol}")),
            None => Err(format!("sous-solution {cible} inconnue")),
        }
    }

    fn nouvel_id(&mut self, id: IdMolecule) -> Result<(), String> {
        if self.vus.insert(id) {
            Ok(())
        } else {
            Err(format!("identifiant de molecule {id} reutilise"))
        }
    }

    fn supprimer_recursivement(&mut self, sol: IdSolution) {
        if let Some(noeud) = self.solutions.remove(&sol) {
            for enfant in noeud.enfants {
                self.supprimer_recursivement(enfant);
            }
        }
    }

    /// Une sous-solution liee (dissoute ou detruite) n'a jamais de courrier
    /// en attente : le moteur ferme sa boite avant de la lier.
    fn sans_courrier(&self, n: u64, cible: IdSolution) -> Result<(), String> {
        match self.solutions.get(&cible) {
            Some(noeud) if !noeud.boite.is_empty() => Err(format!(
                "reaction {n} : {cible} liee alors que {} molecule(s) lui sont en transit",
                noeud.boite.len()
            )),
            _ => Ok(()),
        }
    }

    /// Applique un evenement ; refuse tout evenement incoherent.
    pub fn appliquer(&mut self, evenement: &Evenement) -> Result<(), String> {
        match evenement {
            Evenement::Debut { graine, .. } => self.graine = Some(*graine),
            Evenement::Init {
                sol,
                parent,
                regles,
                molecules,
                ..
            } => {
                if self.solutions.contains_key(sol) {
                    return Err(format!("solution {sol} initialisee deux fois"));
                }
                match parent {
                    None if self.racine.is_some() => {
                        return Err("deux solutions principales".to_string());
                    }
                    None => self.racine = Some(*sol),
                    Some(p) => {
                        self.noeud(*p)?.enfants.insert(*sol);
                    }
                }
                let mut noeud = Noeud {
                    parent: *parent,
                    regles: regles.clone(),
                    ..Noeud::default()
                };
                for m in molecules {
                    self.nouvel_id(m.id)?;
                    noeud.molecules.insert(m.id, m.valeur.clone());
                }
                self.solutions.insert(*sol, noeud);
            }
            Evenement::Reaction {
                sol,
                n,
                regle,
                entrees,
                entrees_sol,
                sorties,
                retiree,
                ..
            } => {
                for cible in entrees_sol {
                    self.enfant_de(*sol, *cible)?;
                }
                // Validation complete avant toute modification.
                let noeud = self
                    .solutions
                    .get(sol)
                    .ok_or_else(|| format!("solution {sol} inconnue"))?;
                let Some(rang_regle) = noeud.regles.iter().position(|r| **r == **regle) else {
                    return Err(format!(
                        "reaction {n} : la regle '{regle}' n'est pas (ou plus) active dans {sol}"
                    ));
                };
                let mut distinctes = HashSet::new();
                for id in entrees {
                    if !noeud.molecules.contains_key(id) || !distinctes.insert(*id) {
                        return Err(format!(
                            "reaction {n} : molecule {id} absente de {sol} (deja consommee ?)"
                        ));
                    }
                }
                for m in sorties {
                    self.nouvel_id(m.id)?;
                }
                let noeud = self.noeud(*sol)?;
                for id in entrees {
                    noeud.molecules.remove(id);
                }
                for m in sorties {
                    noeud.molecules.insert(m.id, m.valeur.clone());
                }
                if *retiree {
                    noeud.regles.remove(rang_regle);
                }
                noeud.inerte = false;
                self.reactions += 1;
            }
            Evenement::Dissolution {
                sol,
                cible,
                molecules,
                solutions,
                n,
                ..
            } => {
                self.enfant_de(*sol, *cible)?;
                self.sans_courrier(*n, *cible)?;
                let dissoute = self
                    .solutions
                    .remove(cible)
                    .ok_or_else(|| format!("sous-solution {cible} inconnue"))?;
                let annoncees: BTreeSet<IdMolecule> = molecules.iter().copied().collect();
                let reelles: BTreeSet<IdMolecule> = dissoute.molecules.keys().copied().collect();
                let sous_annoncees: BTreeSet<IdSolution> = solutions.iter().copied().collect();
                if annoncees != reelles || sous_annoncees != dissoute.enfants {
                    return Err(format!(
                        "dissolution {n} : contenu annonce different du contenu de {cible}"
                    ));
                }
                for enfant in &dissoute.enfants {
                    self.noeud(*enfant)?.parent = Some(*sol);
                }
                let parent = self.noeud(*sol)?;
                parent.enfants.remove(cible);
                parent.enfants.extend(dissoute.enfants);
                parent.molecules.extend(dissoute.molecules);
            }
            Evenement::Injection {
                sol,
                cible,
                sorties,
                ..
            } => {
                self.enfant_de(*sol, *cible)?;
                for m in sorties {
                    self.nouvel_id(m.id)?;
                }
                let noeud = self.noeud(*cible)?;
                for m in sorties {
                    noeud.molecules.insert(m.id, m.valeur.clone());
                }
                noeud.inerte = false;
            }
            Evenement::Envoi {
                sol,
                cible,
                sorties,
                ..
            } => {
                self.enfant_de(*sol, *cible)?;
                for m in sorties {
                    self.nouvel_id(m.id)?;
                }
                let noeud = self.noeud(*cible)?;
                for m in sorties {
                    noeud.boite.insert(m.id, m.valeur.clone());
                }
            }
            Evenement::Reception { sol, molecules, .. } => {
                let noeud = self.noeud(*sol)?;
                let mut distinctes = HashSet::new();
                for id in molecules {
                    if !noeud.boite.contains_key(id) || !distinctes.insert(*id) {
                        return Err(format!(
                            "reception dans {sol} : molecule {id} jamais envoyee (ou deja recue)"
                        ));
                    }
                }
                for id in molecules {
                    if let Some(valeur) = noeud.boite.remove(id) {
                        noeud.molecules.insert(*id, valeur);
                    }
                }
                if !molecules.is_empty() {
                    noeud.inerte = false;
                }
            }
            Evenement::Destruction { sol, cible, n, .. } => {
                self.enfant_de(*sol, *cible)?;
                self.sans_courrier(*n, *cible)?;
                self.noeud(*sol)?.enfants.remove(cible);
                self.supprimer_recursivement(*cible);
            }
            Evenement::Inertie { sol, .. } => self.noeud(*sol)?.inerte = true,
            Evenement::Fin {
                reactions,
                inerte,
                motif,
                ..
            } => {
                self.fin = Some(FinTrace {
                    reactions: *reactions,
                    inerte: *inerte,
                    motif: motif.clone(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solution::Molecule;
    use std::sync::Arc;

    fn m(id: u64, v: i64) -> Molecule {
        Molecule {
            id: IdMolecule(id),
            valeur: Valeur::Entier(v),
        }
    }

    fn init(sol: u64, parent: Option<u64>, mols: Vec<Molecule>) -> Evenement {
        Evenement::Init {
            t: 0.0,
            sol: IdSolution(sol),
            parent: parent.map(IdSolution),
            regles: vec!["max".into()],
            molecules: mols,
        }
    }

    #[test]
    fn reaction_et_double_consommation() {
        let mut modele = ModeleTrace::new();
        modele
            .appliquer(&init(0, None, vec![m(1, 4), m(2, 9)]))
            .unwrap();
        let r = Evenement::Reaction {
            t: 0.0,
            n: 1,
            w: 0,
            sol: IdSolution(0),
            regle: Arc::from("max"),
            entrees: vec![IdMolecule(2), IdMolecule(1)],
            entrees_sol: vec![],
            sorties: vec![m(3, 9)],
            retiree: false,
        };
        modele.appliquer(&r).unwrap();
        assert_eq!(modele.racine_canonique().unwrap(), "{ 9 | max }");
        let erreur = modele.appliquer(&r).unwrap_err();
        assert!(erreur.contains("absente"), "{erreur}");
    }

    fn reaction(n: u64, sol: u64, regle: &str, retiree: bool) -> Evenement {
        Evenement::Reaction {
            t: 0.0,
            n,
            w: 0,
            sol: IdSolution(sol),
            regle: Arc::from(regle),
            entrees: vec![],
            entrees_sol: vec![],
            sorties: vec![],
            retiree,
        }
    }

    #[test]
    fn regle_a_usage_unique_retiree() {
        let mut modele = ModeleTrace::new();
        modele
            .appliquer(&Evenement::Init {
                t: 0.0,
                sol: IdSolution(0),
                parent: None,
                regles: vec!["une".into(), "max".into(), "une".into()],
                molecules: vec![],
            })
            .unwrap();
        // deux activations : deux usages, puis la regle n'est plus active
        modele.appliquer(&reaction(1, 0, "une", true)).unwrap();
        modele.appliquer(&reaction(2, 0, "une", true)).unwrap();
        assert_eq!(modele.regles(IdSolution(0)).unwrap(), ["max"]);
        let erreur = modele.appliquer(&reaction(3, 0, "une", true)).unwrap_err();
        assert!(erreur.contains("pas (ou plus) active"), "{erreur}");
        assert!(modele.appliquer(&reaction(4, 0, "autre", false)).is_err());
        assert_eq!(modele.racine_canonique().unwrap(), "{ | max }");
    }

    #[test]
    fn envoi_puis_reception() {
        let mut modele = ModeleTrace::new();
        modele.appliquer(&init(0, None, vec![m(1, 10)])).unwrap();
        modele.appliquer(&init(1, Some(0), vec![m(2, 4)])).unwrap();
        modele
            .appliquer(&Evenement::Inertie {
                t: 0.0,
                sol: IdSolution(1),
            })
            .unwrap();
        let envoi = Evenement::Envoi {
            t: 0.0,
            n: 1,
            sol: IdSolution(0),
            cible: IdSolution(1),
            sorties: vec![m(3, 50), m(4, 8)],
        };
        modele.appliquer(&envoi).unwrap();
        // en transit : pas encore dans la sous-solution, qui n'est plus liable
        assert_eq!(modele.nombre_en_transit(), 2);
        assert_eq!(modele.destinataires(), vec![IdSolution(1)]);
        assert!(!modele.est_inerte(IdSolution(1)));
        assert_eq!(
            modele.racine_canonique().unwrap(),
            "{ 10, { 4 | max } | max }"
        );
        // identifiants deja vus ; envoi vers une solution qui n'est pas un enfant
        assert!(modele.appliquer(&envoi).is_err());
        assert!(
            modele
                .appliquer(&Evenement::Envoi {
                    t: 0.0,
                    n: 2,
                    sol: IdSolution(1),
                    cible: IdSolution(0),
                    sorties: vec![m(9, 1)],
                })
                .is_err()
        );
        assert!(
            modele
                .appliquer(&Evenement::Destruction {
                    t: 0.0,
                    n: 3,
                    sol: IdSolution(0),
                    cible: IdSolution(1),
                })
                .is_err(),
            "une sous-solution qui attend du courrier ne peut pas etre liee"
        );
        let reception = |ids: Vec<u64>| Evenement::Reception {
            t: 0.0,
            sol: IdSolution(1),
            molecules: ids.into_iter().map(IdMolecule).collect(),
        };
        assert!(modele.appliquer(&reception(vec![3, 3])).is_err());
        assert!(modele.appliquer(&reception(vec![1])).is_err());
        modele.appliquer(&reception(vec![4])).unwrap();
        modele.appliquer(&reception(vec![3])).unwrap();
        assert_eq!(modele.nombre_en_transit(), 0);
        assert_eq!(
            modele.racine_canonique().unwrap(),
            "{ 10, { 4, 8, 50 | max } | max }"
        );
    }

    #[test]
    fn dissolution_injection_destruction() {
        let mut modele = ModeleTrace::new();
        modele.appliquer(&init(0, None, vec![m(1, 10)])).unwrap();
        modele.appliquer(&init(1, Some(0), vec![m(2, 4)])).unwrap();
        modele.appliquer(&init(2, Some(0), vec![m(3, 5)])).unwrap();
        modele.appliquer(&init(3, Some(1), vec![m(4, 6)])).unwrap();
        modele
            .appliquer(&Evenement::Injection {
                t: 0.0,
                n: 1,
                sol: IdSolution(0),
                cible: IdSolution(1),
                sorties: vec![m(5, 10)],
            })
            .unwrap();
        assert!(!modele.est_inerte(IdSolution(1)));
        modele
            .appliquer(&Evenement::Dissolution {
                t: 0.0,
                n: 2,
                sol: IdSolution(0),
                cible: IdSolution(1),
                molecules: vec![IdMolecule(2), IdMolecule(5)],
                solutions: vec![IdSolution(3)],
            })
            .unwrap();
        assert_eq!(modele.parent(IdSolution(3)), Some(IdSolution(0)));
        modele
            .appliquer(&Evenement::Destruction {
                t: 0.0,
                n: 3,
                sol: IdSolution(0),
                cible: IdSolution(2),
            })
            .unwrap();
        assert_eq!(
            modele.racine_canonique().unwrap(),
            "{ 4, 10, 10, { 6 | max } | max }"
        );
        assert!(
            modele
                .appliquer(&Evenement::Destruction {
                    t: 0.0,
                    n: 4,
                    sol: IdSolution(0),
                    cible: IdSolution(2),
                })
                .is_err()
        );
    }
}
