//! Solutions : multi-ensembles de molecules, eventuellement imbriques.
//!
//! Patron Composite : une [`Solution`] contient des [`Element`]s qui sont soit
//! des molecules (feuilles), soit des sous-solutions (composites). Les
//! operations recursives (taille, inertie, affichage, parcours) traitent les
//! deux uniformement.

mod affichage;
mod boite;
mod constructeur;

pub use affichage::formater_canonique;
pub use boite::BoiteAuxLettres;
pub use constructeur::ConstructeurSolution;

use crate::evaluateur::Valeur;
use crate::moteur::RegleCompilee;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Identifiant stable d'une molecule (unique sur toute l'execution).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IdMolecule(pub u64);

/// Identifiant stable d'une solution (`s0` pour la solution principale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IdSolution(pub u64);

impl fmt::Display for IdMolecule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for IdSolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "s{}", self.0)
    }
}

impl IdSolution {
    /// Analyse la forme textuelle `sN`.
    pub fn depuis_texte(texte: &str) -> Option<Self> {
        texte.strip_prefix('s')?.parse().ok().map(IdSolution)
    }
}

/// Generateur d'identifiants partage entre workers.
///
/// Les molecules sont numerotees a partir de 1, les solutions a partir de 0.
/// L'allocation par blocs limite la contention entre workers.
#[derive(Debug)]
pub struct GenerateurIds {
    molecules: AtomicU64,
    solutions: AtomicU64,
}

impl Default for GenerateurIds {
    fn default() -> Self {
        Self {
            molecules: AtomicU64::new(1),
            solutions: AtomicU64::new(0),
        }
    }
}

impl GenerateurIds {
    /// Nouveau generateur.
    pub fn new() -> Self {
        Self::default()
    }

    /// Identifiant de molecule suivant.
    pub fn molecule(&self) -> IdMolecule {
        IdMolecule(self.molecules.fetch_add(1, Ordering::Relaxed))
    }

    /// Reserve `taille` identifiants consecutifs ; retourne le premier.
    pub fn bloc_molecules(&self, taille: u64) -> u64 {
        self.molecules.fetch_add(taille, Ordering::Relaxed)
    }

    /// Identifiant de solution suivant.
    pub fn solution(&self) -> IdSolution {
        IdSolution(self.solutions.fetch_add(1, Ordering::Relaxed))
    }
}

/// Molecule : valeur dotee d'une identite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Molecule {
    /// Identite stable (deux molecules de meme valeur sont distinctes).
    pub id: IdMolecule,
    /// Valeur portee.
    pub valeur: Valeur,
}

/// Element d'une solution : feuille (molecule) ou composite (sous-solution).
#[derive(Debug, Clone)]
pub enum Element {
    /// Molecule.
    Molecule(Molecule),
    /// Sous-solution.
    Solution(Box<Solution>),
}

/// Solution : multi-ensemble d'elements et regles actives.
#[derive(Debug)]
pub struct Solution {
    pub(crate) id: IdSolution,
    pub(crate) regles: Vec<Arc<RegleCompilee>>,
    pub(crate) elements: Vec<Element>,
    pub(crate) inerte: bool,
    /// Courrier des injections asynchrones (EF-D6), partage avec les
    /// references construites par la solution parente.
    pub(crate) boite: Arc<BoiteAuxLettres>,
}

/// Copie independante : la copie a sa propre boite aux lettres (avec une
/// copie du courrier en attente), aucune reference ne la designe.
impl Clone for Solution {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            regles: self.regles.clone(),
            elements: self.elements.clone(),
            inerte: self.inerte,
            boite: Arc::new(self.boite.copie()),
        }
    }
}

/// Reference a une sous-solution directe (motif `s:ref`) : son identifiant
/// et sa boite aux lettres. Valide tant que la boite n'est pas fermee.
#[derive(Debug, Clone)]
pub struct Port {
    pub(crate) id: IdSolution,
    pub(crate) boite: Arc<BoiteAuxLettres>,
}

impl Port {
    /// Sous-solution designee.
    pub fn id(&self) -> IdSolution {
        self.id
    }

    /// Boite aux lettres de la sous-solution.
    pub fn boite(&self) -> &BoiteAuxLettres {
        &self.boite
    }
}

impl Solution {
    /// Identifiant.
    pub fn id(&self) -> IdSolution {
        self.id
    }

    /// Regles actives (partagees, patron Poids-mouche).
    pub fn regles(&self) -> &[Arc<RegleCompilee>] {
        &self.regles
    }

    /// Noms des regles actives.
    pub fn noms_regles(&self) -> Vec<&str> {
        self.regles.iter().map(|r| r.nom.as_ref()).collect()
    }

    /// Elements directs.
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    /// Molecules directes.
    pub fn molecules(&self) -> impl Iterator<Item = &Molecule> {
        self.elements.iter().filter_map(|e| match e {
            Element::Molecule(m) => Some(m),
            Element::Solution(_) => None,
        })
    }

    /// Sous-solutions directes.
    pub fn sous_solutions(&self) -> impl Iterator<Item = &Solution> {
        self.elements.iter().filter_map(|e| match e {
            Element::Solution(s) => Some(s.as_ref()),
            Element::Molecule(_) => None,
        })
    }

    /// Valeurs directes.
    pub fn valeurs(&self) -> Vec<&Valeur> {
        self.molecules().map(|m| &m.valeur).collect()
    }

    /// Nombre total de molecules, sous-solutions et courrier en attente
    /// compris.
    pub fn nombre_molecules(&self) -> u64 {
        self.boite.nombre() as u64
            + self
                .elements
                .iter()
                .map(|e| match e {
                    Element::Molecule(_) => 1,
                    Element::Solution(s) => s.nombre_molecules(),
                })
                .sum::<u64>()
    }

    /// Boite aux lettres (injections asynchrones).
    pub fn boite(&self) -> &BoiteAuxLettres {
        &self.boite
    }

    /// Reference a cette solution (cible d'injections asynchrones).
    pub fn port(&self) -> Port {
        Port {
            id: self.id,
            boite: Arc::clone(&self.boite),
        }
    }

    /// References a toutes les sous-solutions directes.
    pub fn ports(&self) -> Vec<Port> {
        self.sous_solutions().map(Solution::port).collect()
    }

    /// Molecules en attente dans les boites aux lettres (recursivement).
    pub fn courrier_en_attente(&self) -> u64 {
        self.boite.nombre() as u64
            + self
                .sous_solutions()
                .map(Solution::courrier_en_attente)
                .sum::<u64>()
    }

    /// Nombre total de solutions (celle-ci comprise).
    pub fn nombre_solutions(&self) -> u64 {
        1 + self
            .sous_solutions()
            .map(Solution::nombre_solutions)
            .sum::<u64>()
    }

    /// Inertie au sens du glossaire : aucune regle applicable ici, aucun
    /// courrier en attente et toutes les sous-solutions inertes.
    /// L'indicateur est maintenu par l'ordonnanceur ; cette methode le
    /// consolide recursivement.
    pub fn est_inerte(&self) -> bool {
        self.inerte
            && !self.boite.a_du_courrier()
            && self.sous_solutions().all(Solution::est_inerte)
    }

    /// Parcours en profondeur prefixe (parent avant enfants).
    pub fn parcourir<'a>(&'a self, visite: &mut impl FnMut(&'a Solution, Option<IdSolution>)) {
        self.parcourir_depuis(None, visite);
    }

    fn parcourir_depuis<'a>(
        &'a self,
        parent: Option<IdSolution>,
        visite: &mut impl FnMut(&'a Solution, Option<IdSolution>),
    ) {
        visite(self, parent);
        for enfant in self.sous_solutions() {
            enfant.parcourir_depuis(Some(self.id), visite);
        }
    }

    /// Retire et retourne les sous-solutions actives (non inertes, ou qui
    /// ont du courrier a relever) ; les boites de `Element::Solution` sont
    /// reprises sans reallocation.
    #[allow(clippy::vec_box)]
    pub(crate) fn extraire_sous_solutions_actives(&mut self) -> Vec<Box<Solution>> {
        let mut actives = Vec::new();
        let mut i = 0;
        while i < self.elements.len() {
            if matches!(&self.elements[i], Element::Solution(s) if !s.inerte || s.boite.a_du_courrier())
            {
                if let Element::Solution(s) = self.elements.swap_remove(i) {
                    actives.push(s);
                }
            } else {
                i += 1;
            }
        }
        actives
    }

    /// Reintegre des sous-solutions.
    #[allow(clippy::vec_box)]
    pub(crate) fn reintegrer(&mut self, solutions: Vec<Box<Solution>>) {
        self.elements
            .extend(solutions.into_iter().map(Element::Solution));
    }
}

/// Affichage canonique : valeurs triees, sous-solutions triees, puis regles.
impl fmt::Display for Solution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sous: Vec<String> = self.sous_solutions().map(|s| s.to_string()).collect();
        f.write_str(&formater_canonique(
            self.valeurs(),
            sous,
            &self.noms_regles(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_et_affichage() {
        let ids = GenerateurIds::new();
        let interne = ConstructeurSolution::nouvelle(&ids)
            .entier(17)
            .entier(4)
            .construire();
        let racine = ConstructeurSolution::nouvelle(&ids)
            .entier(3)
            .valeur(Valeur::Booleen(true))
            .sous_solution(interne)
            .entier(-1)
            .construire();
        assert_eq!(racine.to_string(), "{ -1, 3, true, { 4, 17 } }");
        assert_eq!(racine.nombre_molecules(), 5);
        assert_eq!(racine.nombre_solutions(), 2);
        let mut vus = Vec::new();
        racine.parcourir(&mut |s, parent| vus.push((s.id(), parent)));
        assert_eq!(
            vus,
            vec![(IdSolution(1), None), (IdSolution(0), Some(IdSolution(1)))]
        );
        assert!(!racine.est_inerte());
    }

    #[test]
    fn extraction_des_actives() {
        let ids = GenerateurIds::new();
        let mut inerte = ConstructeurSolution::nouvelle(&ids).entier(1).construire();
        inerte.inerte = true;
        let active = ConstructeurSolution::nouvelle(&ids).entier(2).construire();
        let mut racine = ConstructeurSolution::nouvelle(&ids)
            .sous_solution(inerte)
            .entier(5)
            .sous_solution(active)
            .construire();
        let actives = racine.extraire_sous_solutions_actives();
        assert_eq!(actives.len(), 1);
        assert_eq!(racine.elements().len(), 2);
        racine.reintegrer(actives);
        assert_eq!(racine.nombre_molecules(), 3);
    }

    #[test]
    fn courrier_en_attente_reactive() {
        let ids = GenerateurIds::new();
        let mut inerte = ConstructeurSolution::nouvelle(&ids).entier(1).construire();
        inerte.inerte = true;
        let mut racine = ConstructeurSolution::nouvelle(&ids)
            .sous_solution(inerte)
            .construire();
        racine.inerte = true;
        assert!(racine.est_inerte());
        let ports = racine.ports();
        assert_eq!(ports.len(), 1);
        assert!(ports[0].boite().reserver_envoi());
        ports[0].boite().deposer([Molecule {
            id: ids.molecule(),
            valeur: Valeur::Entier(7),
        }]);
        // du courrier attend : ni la sous-solution ni la racine ne sont inertes
        assert!(!racine.est_inerte());
        assert_eq!(racine.nombre_molecules(), 2);
        assert_eq!(racine.courrier_en_attente(), 1);
        // une copie est independante des references existantes
        let copie = racine.clone();
        assert_eq!(copie.courrier_en_attente(), 1);
        assert!(!Arc::ptr_eq(
            &copie.sous_solutions().next().unwrap().boite,
            &ports[0].boite
        ));
        let actives = racine.extraire_sous_solutions_actives();
        assert_eq!(actives.len(), 1);
    }

    #[test]
    fn identifiants() {
        assert_eq!(IdSolution(3).to_string(), "s3");
        assert_eq!(IdSolution::depuis_texte("s12"), Some(IdSolution(12)));
        assert_eq!(IdSolution::depuis_texte("12"), None);
        let ids = GenerateurIds::new();
        assert_eq!(ids.molecule(), IdMolecule(1));
        assert_eq!(ids.bloc_molecules(10), 2);
        assert_eq!(ids.molecule(), IdMolecule(12));
    }
}
