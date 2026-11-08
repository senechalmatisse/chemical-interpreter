//! Rejeu d'une trace.
//!
//! Le rejeu relit une trace JSON Lines et reconstruit l'etat final des
//! solutions avec le [`ModeleTrace`], qui refuse toute incoherence (molecule
//! consommee deux fois, identifiant reutilise...). Si le programme est
//! fourni, chaque reaction est de plus re-executee : motifs, condition,
//! produits et effets sur les sous-solutions (dissolution, injection,
//! destruction, envoi asynchrone) sont recalcules et compares a la trace,
//! ainsi que le retrait des regles a usage unique. Une trace produite en
//! parallele est ainsi validee comme une execution sequentielle possible.

use crate::evaluateur::Valeur;
use crate::moteur::{MotifCompile, ProduitCompile, RegleCompilee};
use crate::solution::IdSolution;
use crate::trace::{Evenement, ModeleTrace};
use std::collections::BTreeMap;
use std::fmt;
use std::io::BufRead;
use std::sync::Arc;

/// Erreur de rejeu, localisee par numero de ligne.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErreurRejeu {
    /// Ligne de la trace (1 = premiere).
    pub ligne: usize,
    /// Explication.
    pub message: String,
    /// Vrai si la ligne est illisible (format), faux si elle est incoherente.
    pub format: bool,
}

impl fmt::Display for ErreurRejeu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ligne {} : {}", self.ligne, self.message)
    }
}

impl std::error::Error for ErreurRejeu {}

/// Resultat d'un rejeu.
#[derive(Debug, Clone)]
pub struct Rejeu {
    /// Etat final reconstruit.
    pub modele: ModeleTrace,
    /// Evenements lus.
    pub evenements: usize,
    /// Reactions re-executees avec succes (0 sans programme).
    pub reactions_verifiees: u64,
}

#[derive(Debug, PartialEq)]
enum Effet {
    Dissolution(IdSolution),
    Injection(IdSolution, Valeur),
    Destruction(IdSolution),
    /// Valeurs envoyees a une meme reference (la cible n'est connue que par
    /// la trace : une sous-solution directe, vivante, distincte des autres).
    Envoi(Vec<Valeur>),
}

struct Attente {
    n: u64,
    sol: IdSolution,
    /// Sous-solutions liees par la reaction (jamais cibles d'un envoi).
    liees: Vec<IdSolution>,
    /// Cibles des envois deja vus (distinctes).
    cibles: Vec<IdSolution>,
    effets: Vec<Effet>,
}

struct Verificateur<'r> {
    regles: &'r BTreeMap<String, Arc<RegleCompilee>>,
    attente: Option<Attente>,
}

impl Verificateur<'_> {
    fn cloturer(&mut self) -> Result<(), String> {
        match self.attente.take() {
            Some(a) if !a.effets.is_empty() => Err(format!(
                "reaction {} : effet(s) attendu(s) absent(s) de la trace : {:?}",
                a.n, a.effets
            )),
            _ => Ok(()),
        }
    }

    fn reaction(&mut self, modele: &ModeleTrace, evenement: &Evenement) -> Result<(), String> {
        let Evenement::Reaction {
            n,
            sol,
            regle,
            entrees,
            entrees_sol,
            sorties,
            retiree,
            ..
        } = evenement
        else {
            return Ok(());
        };
        let (n, sol) = (*n, *sol);
        let regle: &str = regle;
        self.cloturer()?;
        let r = self
            .regles
            .get(regle)
            .ok_or_else(|| format!("reaction {n} : regle '{regle}' absente du programme"))?;
        if *retiree != r.unique {
            return Err(if r.unique {
                format!("reaction {n} : la regle a usage unique '{regle}' n'a pas ete retiree")
            } else {
                format!(
                    "reaction {n} : la regle '{regle}' n'est pas a usage unique, elle ne peut pas etre retiree"
                )
            });
        }
        if r.motifs.len() != entrees.len() + entrees_sol.len() {
            return Err(format!(
                "reaction {n} : {} reactif(s) pour une regle d'arite {}",
                entrees.len() + entrees_sol.len(),
                r.arite()
            ));
        }
        let mut emplacements = vec![Valeur::Entier(0); r.emplacements];
        let mut valeurs = entrees.iter();
        let mut solutions = entrees_sol.iter();
        let mut liees: Vec<Option<IdSolution>> = Vec::with_capacity(r.motifs.len());
        for motif in &r.motifs {
            if *motif == MotifCompile::Solution {
                let cible = *solutions
                    .next()
                    .ok_or_else(|| format!("reaction {n} : sous-solution manquante"))?;
                if modele.parent(cible) != Some(sol) {
                    return Err(format!(
                        "reaction {n} : {cible} n'est pas une sous-solution de {sol}"
                    ));
                }
                if !modele.est_inerte(cible) {
                    return Err(format!(
                        "reaction {n} : {cible} liee alors qu'elle n'est pas inerte (EF-S3)"
                    ));
                }
                liees.push(Some(cible));
            } else {
                let id = *valeurs
                    .next()
                    .ok_or_else(|| format!("reaction {n} : molecule manquante"))?;
                let valeur = modele
                    .valeur(sol, id)
                    .ok_or_else(|| format!("reaction {n} : molecule {id} absente de {sol}"))?;
                if !motif.lier(valeur, &mut emplacements) {
                    return Err(format!(
                        "reaction {n} : la molecule {id} ({valeur}) ne correspond pas au motif"
                    ));
                }
                liees.push(None);
            }
        }
        if let Some(c) = &r.condition {
            match c.evaluer(&emplacements) {
                Ok(Valeur::Booleen(true)) => {}
                Ok(v) => {
                    return Err(format!(
                        "reaction {n} : la condition vaut {v} pour ces reactifs"
                    ));
                }
                Err(e) => return Err(format!("reaction {n} : condition non evaluable ({e})")),
            }
        }
        let mut produites = Vec::new();
        let mut effets = Vec::new();
        let mut utilisees = vec![false; liees.len()];
        let mut envois: Vec<Vec<Valeur>> = vec![Vec::new(); r.references];
        for p in &r.produits {
            let evaluer = |e: &crate::evaluateur::ExprCompilee| {
                e.evaluer(&emplacements)
                    .map_err(|err| format!("reaction {n} : produit non evaluable ({err})"))
            };
            match p {
                ProduitCompile::Valeur(e) => produites.push(evaluer(e)?),
                ProduitCompile::Injection { valeur, reactif } => {
                    utilisees[*reactif] = true;
                    let cible = liees[*reactif].ok_or("reactif d'injection invalide")?;
                    effets.push(Effet::Injection(cible, evaluer(valeur)?));
                }
                ProduitCompile::Dissolution { reactif } => {
                    utilisees[*reactif] = true;
                    effets.push(Effet::Dissolution(
                        liees[*reactif].ok_or("reactif de dissolution invalide")?,
                    ));
                }
                ProduitCompile::Conservation { reactif } => utilisees[*reactif] = true,
                ProduitCompile::Envoi { valeur, reference } => envois
                    .get_mut(*reference)
                    .ok_or("reference d'envoi invalide")?
                    .push(evaluer(valeur)?),
            }
        }
        for (k, liee) in liees.iter().enumerate() {
            if let Some(cible) = liee
                && !utilisees[k]
            {
                effets.push(Effet::Destruction(*cible));
            }
        }
        effets.extend(
            envois
                .into_iter()
                .filter(|v| !v.is_empty())
                .map(Effet::Envoi),
        );
        let annoncees: Vec<&Valeur> = sorties.iter().map(|m| &m.valeur).collect();
        if annoncees != produites.iter().collect::<Vec<_>>() {
            return Err(format!(
                "reaction {n} : produits annonces {:?} au lieu de {:?}",
                annoncees.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
                produites.iter().map(|v| v.to_string()).collect::<Vec<_>>()
            ));
        }
        self.attente = Some(Attente {
            n,
            sol,
            liees: entrees_sol.clone(),
            cibles: Vec::new(),
            effets,
        });
        Ok(())
    }

    /// Envoi annonce par la trace : la cible doit etre une sous-solution
    /// directe et vivante de la solution de la reaction, distincte des
    /// sous-solutions liees et des autres cibles de la meme reaction.
    fn envoi(
        &mut self,
        modele: &ModeleTrace,
        n: u64,
        cible: IdSolution,
        valeurs: Vec<Valeur>,
    ) -> Result<(), String> {
        let attente = self
            .attente
            .as_mut()
            .filter(|a| a.n == n)
            .ok_or_else(|| format!("envoi vers {cible} sans reaction {n} associee"))?;
        if modele.parent(cible) != Some(attente.sol) {
            return Err(format!(
                "reaction {n} : envoi vers {cible}, qui n'est pas une sous-solution de {}",
                attente.sol
            ));
        }
        if attente.liees.contains(&cible) || attente.cibles.contains(&cible) {
            return Err(format!(
                "reaction {n} : {cible} referencee deux fois (references distinctes attendues)"
            ));
        }
        attente.cibles.push(cible);
        self.effet(n, Effet::Envoi(valeurs))
    }

    fn effet(&mut self, n: u64, effet: Effet) -> Result<(), String> {
        let attente = self
            .attente
            .as_mut()
            .filter(|a| a.n == n)
            .ok_or_else(|| format!("effet {effet:?} sans reaction {n} associee"))?;
        match attente.effets.iter().position(|e| *e == effet) {
            Some(i) => {
                attente.effets.remove(i);
                Ok(())
            }
            None => Err(format!(
                "reaction {n} : effet {effet:?} non prevu par la regle"
            )),
        }
    }

    fn evenement(&mut self, modele: &ModeleTrace, e: &Evenement) -> Result<(), String> {
        match e {
            Evenement::Reaction { .. } => self.reaction(modele, e),
            Evenement::Dissolution { n, cible, .. } => self.effet(*n, Effet::Dissolution(*cible)),
            Evenement::Destruction { n, cible, .. } => self.effet(*n, Effet::Destruction(*cible)),
            Evenement::Injection {
                n, cible, sorties, ..
            } => {
                for m in sorties {
                    self.effet(*n, Effet::Injection(*cible, m.valeur.clone()))?;
                }
                Ok(())
            }
            Evenement::Envoi {
                n, cible, sorties, ..
            } => self.envoi(
                modele,
                *n,
                *cible,
                sorties.iter().map(|m| m.valeur.clone()).collect(),
            ),
            _ => self.cloturer(),
        }
    }
}

/// Rejoue une trace ; avec `regles`, re-execute et verifie chaque reaction.
pub fn rejouer(
    lecteur: impl BufRead,
    regles: Option<&BTreeMap<String, Arc<RegleCompilee>>>,
) -> Result<Rejeu, ErreurRejeu> {
    let mut modele = ModeleTrace::new();
    let mut verificateur = regles.map(|regles| Verificateur {
        regles,
        attente: None,
    });
    let mut evenements = 0;
    let mut verifiees = 0;
    let mut derniere = 0;
    for (index, ligne) in lecteur.lines().enumerate() {
        let numero = index + 1;
        derniere = numero;
        let ligne = ligne.map_err(|e| ErreurRejeu {
            ligne: numero,
            message: e.to_string(),
            format: true,
        })?;
        if ligne.trim().is_empty() {
            continue;
        }
        let evenement = Evenement::depuis_ligne(&ligne).map_err(|message| ErreurRejeu {
            ligne: numero,
            message,
            format: true,
        })?;
        let incoherence = |message| ErreurRejeu {
            ligne: numero,
            message,
            format: false,
        };
        if let Some(v) = verificateur.as_mut() {
            v.evenement(&modele, &evenement).map_err(incoherence)?;
            if matches!(evenement, Evenement::Reaction { .. }) {
                verifiees += 1;
            }
        }
        modele.appliquer(&evenement).map_err(incoherence)?;
        if let Evenement::Fin { reactions, .. } = &evenement
            && *reactions != modele.reactions
        {
            return Err(incoherence(format!(
                "la fin annonce {reactions} reaction(s), la trace en contient {}",
                modele.reactions
            )));
        }
        evenements += 1;
    }
    if let Some(v) = verificateur.as_mut() {
        v.cloturer().map_err(|message| ErreurRejeu {
            ligne: derniere,
            message,
            format: false,
        })?;
    }
    if modele.racine().is_none() {
        return Err(ErreurRejeu {
            ligne: derniere.max(1),
            message: "aucune solution initialisee (evenement 'init' absent)".to_string(),
            format: true,
        });
    }
    Ok(Rejeu {
        modele,
        evenements,
        reactions_verifiees: verifiees,
    })
}
