//! Reacteur : fait reagir un lot jusqu'a son inertie locale.
//!
//! Algorithme : passes successives sur le lot. Pour
//! chaque element vivant `i` (l'ancre), on cherche une regle et des
//! partenaires d'indices `> i` formant un n-uplet d'occurrences distinctes
//! qui satisfait motifs et condition ; toutes les permutations du n-uplet
//! sont essayees. Une reaction consomme les reactifs et place les produits
//! dans les emplacements liberes apres l'ancre, puis dans celui de l'ancre
//! (aussitot re-tentee), puis en fin de lot : ils sont vus dans la meme passe
//! et le lot ne grossit pas inutilement. Une passe complete sans
//! reaction prouve qu'aucune combinaison n'est applicable : l'inertie locale
//! est exacte, quelle que soit la strategie.
//!
//! Chaque passe commence par relever le courrier de la solution (injections
//! asynchrones recues pendant qu'elle reagit) et par tenter les regles
//! sans reactif, qui n'ont pas d'ancre.
//!
//! Protocole d'une reaction : selectionner, filtrer, evaluer la condition,
//! evaluer les produits, puis **reserver** dans cet ordre : l'activation
//! d'une regle a usage unique (drapeau atomique partage par tous les lots de
//! la solution), un jeton d'etape, la fermeture des boites aux lettres des
//! sous-solutions liees (elles ne doivent pas recevoir de courrier), les
//! boites des sous-solutions referencees (`s:ref`, distinctes). Un echec
//! annule les reservations deja faites ; une erreur avant la validation
//! laisse le lot intact. La validation (retrait et ajout) ne peut plus
//! echouer.

use super::combinaisons::{Combinaisons, Permutations};
use super::contexte::{Arret, Compteurs, ContexteExecution, ErreurExecution};
use super::lot::Lot;
use super::regle::{MotifCompile, ProduitCompile, RegleCompilee};
use crate::alea::Alea;
use crate::evaluateur::{ErreurEvaluation, Valeur};
use crate::solution::{
    BoiteAuxLettres, Element, GenerateurIds, IdMolecule, IdSolution, Molecule, Port, Solution,
};
use crate::source::Span;
use crate::trace::{Diffuseur, Evenement};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Jetons d'etape preleves a la fois sur le budget global.
const BLOC_JETONS: u64 = 256;
/// Identifiants de molecules reserves a la fois.
const BLOC_IDS: u64 = 1024;
/// Nombre de tentatives entre deux points de controle (echeance, arret).
const PERIODE_CONTROLE: u32 = 1024;

enum ProduitEvalue {
    Valeur(Valeur),
    Dissolution(usize),
    Injection(Valeur, usize),
    Conservation(usize),
    Envoi(Valeur, usize),
}

/// Etat propre a un worker pendant le traitement d'un lot (jamais partage).
struct EtatLocal {
    worker: usize,
    jetons: u64,
    prochain_id: u64,
    fin_ids: u64,
    delta_molecules: i64,
    tic: u32,
    par_regle: Vec<u64>,
    compteurs: Compteurs,
    tampon: Vec<Evenement>,
    taille_lot: usize,
    emplacements: Vec<Valeur>,
    produits: Vec<ProduitEvalue>,
    solutions_liees: Vec<Option<Box<Solution>>>,
    emplacements_libres: Vec<usize>,
    /// Motifs `s:sol` de la reaction en cours dont la boite a ete fermee.
    fermees: Vec<usize>,
    /// Ports reserves par la reaction en cours (un par reference).
    ports_choisis: Vec<usize>,
    /// Molecules envoyees par la reaction en cours (une liste par reference).
    envois: Vec<Vec<Molecule>>,
    /// Boites a rouvrir apres la validation (sous-solutions conservees).
    a_rouvrir: Vec<Arc<BoiteAuxLettres>>,
    alea: Option<Alea>,
}

impl EtatLocal {
    #[inline]
    fn controler(&mut self, ctx: &ContexteExecution<'_>) -> Result<(), Arret> {
        self.tic += 1;
        if self.tic >= PERIODE_CONTROLE {
            self.tic = 0;
            ctx.garde.verifier()?;
        }
        Ok(())
    }

    fn prendre_jeton(&mut self, ctx: &ContexteExecution<'_>) -> Result<(), Arret> {
        ctx.garde.controle_rapide()?;
        if self.jetons == 0 {
            ctx.garde.verifier()?;
            ctx.garde
                .ajuster_molecules(std::mem::take(&mut self.delta_molecules))?;
            self.jetons = ctx.garde.prendre_jetons(BLOC_JETONS);
            if self.jetons == 0 {
                return Err(ctx
                    .garde
                    .signaler(Arret::MaxEtapes(ctx.garde.limite_etapes())));
            }
        }
        self.jetons -= 1;
        Ok(())
    }

    fn nouvel_id(&mut self, ids: &GenerateurIds) -> IdMolecule {
        if self.prochain_id == self.fin_ids {
            self.prochain_id = ids.bloc_molecules(BLOC_IDS);
            self.fin_ids = self.prochain_id + BLOC_IDS;
        }
        let id = self.prochain_id;
        self.prochain_id += 1;
        IdMolecule(id)
    }

    fn publier(&mut self, evenement: Evenement, annexes: Vec<Evenement>, diffuseur: &Diffuseur) {
        self.tampon.push(evenement);
        self.tampon.extend(annexes);
        if self.tampon.len() >= self.taille_lot {
            self.diffuser(diffuseur);
        }
    }

    /// Diffuse aussitot les evenements en attente.
    fn diffuser(&mut self, diffuseur: &Diffuseur) {
        diffuseur.diffuser_lot(&self.tampon);
        self.tampon.clear();
    }
}

fn element(lot: &Lot, i: usize) -> &Element {
    lot.elements[i]
        .as_ref()
        .expect("invariant : les indices tentes designent des elements vivants")
}

/// Fait reagir les lots d'une solution donnee.
///
/// Un reacteur est partage (par reference) par tous les lots d'une meme
/// phase : les drapeaux des regles a usage unique valent pour la solution
/// entiere.
pub struct Reacteur<'c, 'a> {
    ctx: &'c ContexteExecution<'a>,
    solution: IdSolution,
    regles: &'c [Arc<RegleCompilee>],
    emplacements: usize,
    /// Un drapeau par activation de regle a usage unique (leve a sa reaction).
    uniques: Vec<Option<AtomicBool>>,
    /// Regles sans reactif (tentees une fois par passe).
    sans_reactif: Vec<usize>,
    /// Sous-solutions directes, cibles possibles des references `s:ref`.
    ports: &'c [Port],
    /// Boite aux lettres de la solution (courrier releve a chaque passe).
    boite: Option<&'c BoiteAuxLettres>,
}

impl<'c, 'a> Reacteur<'c, 'a> {
    /// Reacteur pour les regles d'une solution.
    pub fn new(
        ctx: &'c ContexteExecution<'a>,
        solution: IdSolution,
        regles: &'c [Arc<RegleCompilee>],
    ) -> Self {
        let emplacements = regles.iter().map(|r| r.emplacements).max().unwrap_or(0);
        Self {
            ctx,
            solution,
            regles,
            emplacements,
            uniques: regles
                .iter()
                .map(|r| r.unique.then(|| AtomicBool::new(false)))
                .collect(),
            sans_reactif: (0..regles.len())
                .filter(|&r| regles[r].sans_reactif())
                .collect(),
            ports: &[],
            boite: None,
        }
    }

    /// Sous-solutions referencables. Des que la liste n'est pas vide, les
    /// sous-solutions liees par `s:sol` sont reservees contre les envois
    /// concurrents (fermeture de leur boite).
    pub fn avec_ports(mut self, ports: &'c [Port]) -> Self {
        self.ports = ports;
        self
    }

    /// Boite aux lettres de la solution, relevee au debut de chaque passe.
    pub fn avec_boite(mut self, boite: &'c BoiteAuxLettres) -> Self {
        self.boite = Some(boite);
        self
    }

    /// Vrai si l'activation `r`, a usage unique, a deja reagi.
    pub fn retiree(&self, r: usize) -> bool {
        self.uniques[r]
            .as_ref()
            .is_some_and(|d| d.load(Ordering::Acquire))
    }

    /// Regle encore applicable en principe : pas deja retiree, et assez de
    /// sous-solutions pour ses references.
    #[inline]
    fn disponible(&self, r: usize) -> bool {
        self.regles[r].references <= self.ports.len() && !self.retiree(r)
    }

    /// Fait reagir le lot jusqu'a son inertie locale ; retourne le nombre de
    /// reactions. En cas d'arret, le lot reste coherent (aucune molecule
    /// perdue ni dupliquee) et les evenements deja produits sont publies.
    pub fn reagir(&self, lot: &mut Lot, worker: usize) -> Result<u64, Arret> {
        let mut local = EtatLocal {
            worker,
            jetons: 0,
            prochain_id: 0,
            fin_ids: 0,
            delta_molecules: 0,
            tic: 0,
            par_regle: vec![0; self.regles.len()],
            compteurs: Compteurs::default(),
            tampon: Vec::new(),
            taille_lot: self.ctx.diffuseur.taille_lot(),
            emplacements: vec![Valeur::Entier(0); self.emplacements],
            produits: Vec::new(),
            solutions_liees: Vec::new(),
            emplacements_libres: Vec::new(),
            fermees: Vec::new(),
            ports_choisis: Vec::new(),
            envois: Vec::new(),
            a_rouvrir: Vec::new(),
            alea: self.ctx.melanger.then(|| Alea::new(lot.graine)),
        };
        let resultat = self.boucle(lot, &mut local);
        let vidage = self.vider(&mut local);
        let reactions = resultat?;
        vidage?;
        Ok(reactions)
    }

    fn vider(&self, local: &mut EtatLocal) -> Result<(), Arret> {
        if !local.tampon.is_empty() {
            local.diffuser(self.ctx.diffuseur);
        }
        self.ctx
            .garde
            .rendre_jetons(std::mem::take(&mut local.jetons));
        for (regle, &n) in self.regles.iter().zip(&local.par_regle) {
            if n > 0 {
                *local
                    .compteurs
                    .par_regle
                    .entry(regle.nom.to_string())
                    .or_default() += n;
            }
        }
        if local.compteurs.reactions > 0 {
            local
                .compteurs
                .par_worker
                .insert(local.worker, local.compteurs.reactions);
        }
        self.ctx.ajouter_compteurs(&local.compteurs);
        self.ctx
            .garde
            .ajuster_molecules(std::mem::take(&mut local.delta_molecules))
    }

    fn boucle(&self, lot: &mut Lot, local: &mut EtatLocal) -> Result<u64, Arret> {
        if self.regles.is_empty() {
            return Ok(0);
        }
        let mut total = 0;
        loop {
            lot.compacter();
            self.recevoir(lot, local);
            if let Some(alea) = local.alea.as_mut() {
                alea.melanger(&mut lot.elements);
            }
            local.compteurs.passes += 1;
            let reactions = self.passe(lot, local)?;
            total += reactions;
            if reactions == 0 {
                return Ok(total);
            }
        }
    }

    /// Releve le courrier de la solution : les molecules recues rejoignent
    /// le lot avant la passe, qui les tente donc comme les autres.
    fn recevoir(&self, lot: &mut Lot, local: &mut EtatLocal) {
        let Some(boite) = self.boite else { return };
        let recues = boite.relever();
        if recues.is_empty() {
            return;
        }
        local.compteurs.receptions += recues.len() as u64;
        if self.ctx.diffuseur.est_actif() {
            let reception = Evenement::Reception {
                t: self.ctx.temps(),
                sol: self.solution,
                molecules: recues.iter().map(|m| m.id).collect(),
            };
            local.publier(reception, Vec::new(), self.ctx.diffuseur);
        }
        lot.elements
            .extend(recues.into_iter().map(|m| Some(Element::Molecule(m))));
    }

    fn passe(&self, lot: &mut Lot, local: &mut EtatLocal) -> Result<u64, Arret> {
        let mut reactions = 0;
        // Regles sans reactif : aucune ancre, une tentative par passe.
        let n = self.sans_reactif.len();
        if n > 0 {
            let depart = match local.alea.as_mut() {
                Some(alea) if n > 1 => alea.indice(n),
                _ => 0,
            };
            for k in 0..n {
                let r = self.sans_reactif[(depart + k) % n];
                if self.disponible(r) {
                    local.controler(self.ctx)?;
                    if self.tenter(lot, &[], r, local)? {
                        reactions += 1;
                    }
                }
            }
        }
        let nombre = self.regles.len();
        let mut i = 0;
        while i < lot.elements.len() {
            let mut reaction = false;
            if lot.elements[i].is_some() {
                let depart = match local.alea.as_mut() {
                    Some(alea) if nombre > 1 => alea.indice(nombre),
                    _ => 0,
                };
                for k in 0..nombre {
                    let r = (depart + k) % nombre;
                    if self.regles[r].sans_reactif() || !self.disponible(r) {
                        continue;
                    }
                    if self.tenter_ancre(lot, i, r, local)? {
                        reactions += 1;
                        reaction = true;
                        break;
                    }
                }
            }
            // Un produit place a l'emplacement de l'ancre est tente aussitot.
            if !(reaction && lot.elements[i].is_some()) {
                i += 1;
            }
        }
        Ok(reactions)
    }

    fn tenter_ancre(
        &self,
        lot: &mut Lot,
        i: usize,
        r: usize,
        local: &mut EtatLocal,
    ) -> Result<bool, Arret> {
        let regle = &*self.regles[r];
        match regle.motifs.len() {
            1 => {
                local.controler(self.ctx)?;
                if !regle.motifs[0].accepte(element(lot, i)) {
                    return Ok(false);
                }
                self.tenter(lot, &[i], r, local)
            }
            2 => {
                let (premier, second) = {
                    let e = element(lot, i);
                    (regle.motifs[0].accepte(e), regle.motifs[1].accepte(e))
                };
                if !premier && !second {
                    return Ok(false);
                }
                for j in i + 1..lot.elements.len() {
                    let (avant, apres) = match &lot.elements[j] {
                        None => continue,
                        Some(e) => (
                            premier && regle.motifs[1].accepte(e),
                            second && regle.motifs[0].accepte(e),
                        ),
                    };
                    local.controler(self.ctx)?;
                    if avant && self.tenter(lot, &[i, j], r, local)? {
                        return Ok(true);
                    }
                    if apres && self.tenter(lot, &[j, i], r, local)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            arite => self.tenter_n_uplets(lot, i, r, arite, local),
        }
    }

    /// Regles d'arite >= 3 : combinaisons de partenaires puis permutations.
    fn tenter_n_uplets(
        &self,
        lot: &mut Lot,
        i: usize,
        r: usize,
        arite: usize,
        local: &mut EtatLocal,
    ) -> Result<bool, Arret> {
        let regle = &*self.regles[r];
        if !regle.motifs.iter().any(|m| m.accepte(element(lot, i))) {
            return Ok(false);
        }
        let partenaires: Vec<usize> = (i + 1..lot.elements.len())
            .filter(|&j| lot.elements[j].is_some())
            .collect();
        for combinaison in Combinaisons::new(partenaires.len(), arite - 1) {
            let mut n_uplet = Vec::with_capacity(arite);
            n_uplet.push(i);
            n_uplet.extend(combinaison.iter().map(|&c| partenaires[c]));
            for permutation in Permutations::new(n_uplet) {
                local.controler(self.ctx)?;
                let compatible = regle
                    .motifs
                    .iter()
                    .zip(&permutation)
                    .all(|(m, &idx)| m.accepte(element(lot, idx)));
                if compatible && self.tenter(lot, &permutation, r, local)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn echec(
        &self,
        lot: &Lot,
        indices: &[usize],
        regle: &RegleCompilee,
        mut erreur: ErreurEvaluation,
    ) -> Arret {
        if erreur.span == Span::default() {
            erreur.span = regle.span;
        }
        let reactifs = indices
            .iter()
            .map(|&i| match element(lot, i) {
                Element::Molecule(m) => m.valeur.to_string(),
                Element::Solution(s) => s.id().to_string(),
            })
            .collect();
        self.ctx.garde.signaler(Arret::Erreur(ErreurExecution {
            regle: regle.nom.to_string(),
            solution: self.solution,
            reactifs,
            erreur,
        }))
    }

    /// Tente la regle `r` sur les elements `indices` (ordre des motifs).
    fn tenter(
        &self,
        lot: &mut Lot,
        indices: &[usize],
        r: usize,
        local: &mut EtatLocal,
    ) -> Result<bool, Arret> {
        let regle = &*self.regles[r];
        for (motif, &idx) in regle.motifs.iter().zip(indices) {
            if !motif.filtrer(element(lot, idx), &mut local.emplacements) {
                return Ok(false);
            }
        }
        if let Some(condition) = &regle.condition {
            match condition.evaluer(&local.emplacements) {
                Ok(Valeur::Booleen(true)) => {}
                Ok(Valeur::Booleen(false)) => return Ok(false),
                Ok(autre) => {
                    let erreur =
                        ErreurEvaluation::condition_non_booleenne(&autre, condition.span());
                    return Err(self.echec(lot, indices, regle, erreur));
                }
                Err(erreur) => return Err(self.echec(lot, indices, regle, erreur)),
            }
        }
        let mut produits = std::mem::take(&mut local.produits);
        produits.clear();
        for produit in &regle.produits {
            let evalue = match produit {
                ProduitCompile::Valeur(e) => match e.evaluer(&local.emplacements) {
                    Ok(v) => ProduitEvalue::Valeur(v),
                    Err(erreur) => return Err(self.echec(lot, indices, regle, erreur)),
                },
                ProduitCompile::Injection { valeur, reactif } => {
                    match valeur.evaluer(&local.emplacements) {
                        Ok(v) => ProduitEvalue::Injection(v, *reactif),
                        Err(erreur) => return Err(self.echec(lot, indices, regle, erreur)),
                    }
                }
                ProduitCompile::Envoi { valeur, reference } => {
                    match valeur.evaluer(&local.emplacements) {
                        Ok(v) => ProduitEvalue::Envoi(v, *reference),
                        Err(erreur) => return Err(self.echec(lot, indices, regle, erreur)),
                    }
                }
                ProduitCompile::Dissolution { reactif } => ProduitEvalue::Dissolution(*reactif),
                ProduitCompile::Conservation { reactif } => ProduitEvalue::Conservation(*reactif),
            };
            produits.push(evalue);
        }
        let reserve = self.reserver(lot, indices, r, local);
        if !matches!(reserve, Ok(true)) {
            local.produits = produits;
            return reserve;
        }
        self.valider(lot, indices, r, &mut produits, local);
        local.produits = produits;
        Ok(true)
    }

    /// Reservations prealables a la validation, dans l'ordre : usage unique,
    /// jeton d'etape, fermeture des sous-solutions liees, references. Faux
    /// (toutes reservations annulees) si la reaction n'est plus possible.
    fn reserver(
        &self,
        lot: &Lot,
        indices: &[usize],
        r: usize,
        local: &mut EtatLocal,
    ) -> Result<bool, Arret> {
        let regle = &*self.regles[r];
        if let Some(drapeau) = &self.uniques[r]
            && drapeau
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            // Une autre reaction (eventuellement d'un autre lot) l'a utilisee.
            return Ok(false);
        }
        if let Err(arret) = local.prendre_jeton(self.ctx) {
            if let Some(drapeau) = &self.uniques[r] {
                drapeau.store(false, Ordering::Release);
            }
            return Err(arret);
        }
        local.fermees.clear();
        local.ports_choisis.clear();
        // Sans reference dans la solution, personne ne peut envoyer de
        // courrier a ses sous-solutions : inutile de fermer leurs boites.
        if !self.ports.is_empty() {
            for (k, motif) in regle.motifs.iter().enumerate() {
                if !matches!(motif, MotifCompile::Solution) {
                    continue;
                }
                let Element::Solution(s) = element(lot, indices[k]) else {
                    continue;
                };
                if !s.boite.fermer_si_libre() {
                    // Courrier en attente ou envoi en cours : pas inerte.
                    self.annuler(lot, indices, r, local);
                    return Ok(false);
                }
                local.fermees.push(k);
            }
        }
        let n = self.ports.len();
        if regle.references > 0 {
            let depart = match local.alea.as_mut() {
                Some(alea) if n > 1 => alea.indice(n),
                _ => 0,
            };
            for _ in 0..regle.references {
                // Une boite fermee designe une sous-solution liee par cette
                // reaction ou par une autre, dissoute ou detruite.
                let choisi = (0..n).map(|k| (depart + k) % n).find(|p| {
                    !local.ports_choisis.contains(p) && self.ports[*p].boite.reserver_envoi()
                });
                match choisi {
                    Some(p) => local.ports_choisis.push(p),
                    None => {
                        self.annuler(lot, indices, r, local);
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    /// Annule les reservations d'une reaction abandonnee.
    fn annuler(&self, lot: &Lot, indices: &[usize], r: usize, local: &mut EtatLocal) {
        for &p in &local.ports_choisis {
            self.ports[p].boite.annuler_envoi();
        }
        local.ports_choisis.clear();
        for &k in &local.fermees {
            if let Element::Solution(s) = element(lot, indices[k]) {
                s.boite.rouvrir();
            }
        }
        local.fermees.clear();
        local.jetons += 1;
        if let Some(drapeau) = &self.uniques[r] {
            drapeau.store(false, Ordering::Release);
        }
    }

    /// Validation : retrait des reactifs, ajout des produits, envois,
    /// evenements. Ne peut pas echouer.
    fn valider(
        &self,
        lot: &mut Lot,
        indices: &[usize],
        r: usize,
        produits: &mut Vec<ProduitEvalue>,
        local: &mut EtatLocal,
    ) {
        let regle = &*self.regles[r];
        let tracer = self.ctx.diffuseur.est_actif();
        // Boites fermees par `reserver` : celles des survivantes seront rouvertes.
        let boites_fermees = !local.fermees.is_empty();
        let mut entrees = Vec::new();
        let mut entrees_sol = Vec::new();
        local.solutions_liees.clear();
        local.solutions_liees.resize_with(indices.len(), || None);
        for (k, &idx) in indices.iter().enumerate() {
            match lot.elements[idx].take() {
                Some(Element::Molecule(m)) => {
                    local.delta_molecules -= 1;
                    if tracer {
                        entrees.push(m.id);
                    }
                }
                Some(Element::Solution(s)) => {
                    if tracer {
                        entrees_sol.push(s.id());
                    }
                    local.solutions_liees[k] = Some(s);
                }
                None => debug_assert!(false, "reactif deja consomme"),
            }
        }
        // Les emplacements liberes apres l'ancre (indices > ancre) accueillent
        // les premiers elements ajoutes : ils seront vus plus loin dans la meme
        // passe, sans faire grossir le lot. Les suivants vont en fin de lot.
        let ancre = indices.iter().copied().min().unwrap_or(0);
        let mut libres = std::mem::take(&mut local.emplacements_libres);
        libres.clear();
        libres.extend(indices.iter().copied().filter(|&k| k != ancre));
        libres.sort_unstable_by(|a, b| b.cmp(a));
        // Une regle sans reactif n'a pas d'ancre : ses produits vont en fin de lot.
        let mut ancre_libre = !indices.is_empty();
        let mut placer = |lot: &mut Lot, e: Element| match libres.pop() {
            Some(k) => lot.elements[k] = Some(e),
            // Puis l'emplacement de l'ancre : la passe re-tente cette ancre.
            None if ancre_libre => {
                ancre_libre = false;
                lot.elements[ancre] = Some(e);
            }
            None => lot.elements.push(Some(e)),
        };
        let (numero, t) = if tracer {
            (self.ctx.numero_suivant(), self.ctx.temps())
        } else {
            (0, 0.0)
        };
        let mut sorties = Vec::new();
        let mut annexes = Vec::new();
        local.envois.resize_with(regle.references, Vec::new);
        for produit in produits.drain(..) {
            match produit {
                ProduitEvalue::Valeur(valeur) => {
                    let m = Molecule {
                        id: local.nouvel_id(self.ctx.ids),
                        valeur,
                    };
                    if tracer {
                        sorties.push(m.clone());
                    }
                    placer(lot, Element::Molecule(m));
                    local.delta_molecules += 1;
                }
                ProduitEvalue::Dissolution(k) => {
                    if let Some(dissoute) = local.solutions_liees[k].take() {
                        let Solution { id, elements, .. } = *dissoute;
                        let mut molecules = Vec::new();
                        let mut solutions = Vec::new();
                        for e in elements {
                            if tracer {
                                match &e {
                                    Element::Molecule(m) => molecules.push(m.id),
                                    Element::Solution(s) => solutions.push(s.id()),
                                }
                            }
                            placer(lot, e);
                        }
                        local.compteurs.dissolutions += 1;
                        if tracer {
                            annexes.push(Evenement::Dissolution {
                                t,
                                n: numero,
                                sol: self.solution,
                                cible: id,
                                molecules,
                                solutions,
                            });
                        }
                    }
                }
                ProduitEvalue::Injection(valeur, k) => {
                    if let Some(mut cible) = local.solutions_liees[k].take() {
                        let m = Molecule {
                            id: local.nouvel_id(self.ctx.ids),
                            valeur,
                        };
                        if tracer {
                            annexes.push(Evenement::Injection {
                                t,
                                n: numero,
                                sol: self.solution,
                                cible: cible.id(),
                                sorties: vec![m.clone()],
                            });
                        }
                        cible.elements.push(Element::Molecule(m));
                        cible.inerte = false;
                        local.delta_molecules += 1;
                        local.compteurs.injections += 1;
                        if boites_fermees {
                            local.a_rouvrir.push(Arc::clone(&cible.boite));
                        }
                        lot.reactivees.push(cible);
                    }
                }
                ProduitEvalue::Conservation(k) => {
                    if let Some(s) = local.solutions_liees[k].take() {
                        if boites_fermees {
                            local.a_rouvrir.push(Arc::clone(&s.boite));
                        }
                        placer(lot, Element::Solution(s));
                    }
                }
                ProduitEvalue::Envoi(valeur, k) => {
                    let m = Molecule {
                        id: local.nouvel_id(self.ctx.ids),
                        valeur,
                    };
                    local.envois[k].push(m);
                    local.delta_molecules += 1;
                    local.compteurs.envois += 1;
                }
            }
        }
        for liee in local.solutions_liees.iter_mut() {
            if let Some(detruite) = liee.take() {
                local.delta_molecules -= detruite.nombre_molecules() as i64;
                local.compteurs.destructions += 1;
                if tracer {
                    annexes.push(Evenement::Destruction {
                        t,
                        n: numero,
                        sol: self.solution,
                        cible: detruite.id(),
                    });
                }
            }
        }
        let envoi = local.envois.iter().any(|e| !e.is_empty());
        if tracer && envoi {
            for (k, molecules) in local.envois.iter().enumerate() {
                if !molecules.is_empty() {
                    annexes.push(Evenement::Envoi {
                        t,
                        n: numero,
                        sol: self.solution,
                        cible: self.ports[local.ports_choisis[k]].id,
                        sorties: molecules.clone(),
                    });
                }
            }
        }
        local.emplacements_libres = libres;
        local.par_regle[r] += 1;
        local.compteurs.reactions += 1;
        if regle.unique {
            local.compteurs.regles_retirees += 1;
        }
        if tracer {
            let reaction = Evenement::Reaction {
                t,
                n: numero,
                w: local.worker,
                sol: self.solution,
                regle: Arc::clone(&regle.nom),
                entrees,
                entrees_sol,
                sorties,
                retiree: regle.unique,
            };
            local.publier(reaction, annexes, self.ctx.diffuseur);
            // Ordre causal de la trace : la reaction et ses envois sont
            // publies avant que le courrier puisse etre releve, et avant
            // qu'une sous-solution conservee puisse recevoir du courrier.
            if envoi || !local.a_rouvrir.is_empty() {
                local.diffuser(self.ctx.diffuseur);
            }
        }
        for (k, &p) in local.ports_choisis.iter().enumerate() {
            let boite = &self.ports[p].boite;
            if local.envois[k].is_empty() {
                boite.annuler_envoi();
            } else {
                boite.deposer(local.envois[k].drain(..));
            }
        }
        local.ports_choisis.clear();
        for boite in local.a_rouvrir.drain(..) {
            boite.rouvrir();
        }
        local.fermees.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moteur::GardeFous;
    use crate::parser::analyser_programme;
    use crate::semantique::compiler;
    use crate::source::Source;
    use std::time::Instant;

    fn regles(texte: &str) -> Vec<Arc<RegleCompilee>> {
        let p = analyser_programme(&Source::new("t", texte)).unwrap();
        compiler(&p, 1000).unwrap().regles.into_values().collect()
    }

    fn molecule(ids: &GenerateurIds, v: i64) -> Molecule {
        Molecule {
            id: ids.molecule(),
            valeur: Valeur::Entier(v),
        }
    }

    fn valeurs(lot: Lot) -> Vec<i64> {
        let mut v: Vec<i64> = lot
            .en_elements()
            .into_iter()
            .filter_map(|e| match e {
                Element::Molecule(m) => m.valeur.comme_entier(),
                Element::Solution(_) => None,
            })
            .collect();
        v.sort_unstable();
        v
    }

    fn contexte<'a>(ids: &'a GenerateurIds, diffuseur: &'a Diffuseur) -> ContexteExecution<'a> {
        let debut = Instant::now();
        ContexteExecution::new(
            GardeFous::new(None, None, None, 0, None, debut),
            ids,
            diffuseur,
            false,
            debut,
        )
    }

    #[test]
    fn courrier_releve_au_debut_de_chaque_passe() {
        let ids = GenerateurIds::new();
        let diffuseur = Diffuseur::new();
        let ctx = contexte(&ids, &diffuseur);
        let regles = regles("let somme = replace x, y by x + y; { | somme }");
        let boite = BoiteAuxLettres::new();
        assert!(boite.reserver_envoi());
        boite.deposer([molecule(&ids, 5), molecule(&ids, 6)]);
        let reacteur = Reacteur::new(&ctx, IdSolution(0), &regles).avec_boite(&boite);
        let mut lot = Lot::new(vec![Element::Molecule(molecule(&ids, 1))], 0);
        assert_eq!(reacteur.reagir(&mut lot, 0).unwrap(), 2);
        assert!(!boite.a_du_courrier());
        assert_eq!(valeurs(lot), vec![12]);
        assert_eq!(ctx.compteurs().receptions, 2);
    }

    #[test]
    fn usage_unique_partage_par_les_lots_d_une_phase() {
        let ids = GenerateurIds::new();
        let diffuseur = Diffuseur::new();
        let ctx = contexte(&ids, &diffuseur);
        let regles = regles("let r = replace-one x by x + 100; { | r }");
        let reacteur = Reacteur::new(&ctx, IdSolution(0), &regles);
        let mut a = Lot::new(vec![Element::Molecule(molecule(&ids, 1))], 0);
        let mut b = Lot::new(vec![Element::Molecule(molecule(&ids, 2))], 1);
        let n = reacteur.reagir(&mut a, 0).unwrap() + reacteur.reagir(&mut b, 1).unwrap();
        assert_eq!(n, 1);
        assert!(reacteur.retiree(0));
        let mut tout = valeurs(a);
        tout.extend(valeurs(b));
        tout.sort_unstable();
        assert_eq!(tout, vec![2, 101]);
    }

    #[test]
    fn regle_sans_reactif_produit_en_fin_de_lot() {
        let ids = GenerateurIds::new();
        let diffuseur = Diffuseur::new();
        let ctx = contexte(&ids, &diffuseur);
        let regles = regles("let a = replace-one by 7, 8; { | a }");
        let reacteur = Reacteur::new(&ctx, IdSolution(0), &regles);
        let mut lot = Lot::new(vec![Element::Molecule(molecule(&ids, 1))], 0);
        assert_eq!(reacteur.reagir(&mut lot, 0).unwrap(), 1);
        // l'element existant n'est pas ecrase
        assert_eq!(valeurs(lot), vec![1, 7, 8]);
        let mut vide = Lot::new(Vec::new(), 0);
        assert_eq!(reacteur.reagir(&mut vide, 0).unwrap(), 0, "deja utilisee");
    }

    #[test]
    fn envois_vers_les_seules_boites_ouvertes() {
        let ids = GenerateurIds::new();
        let diffuseur = Diffuseur::new();
        let ctx = contexte(&ids, &diffuseur);
        let regles = regles("let t = replace x:int, s:ref by inject(x, s); { | t }");
        let ouverte = Arc::new(BoiteAuxLettres::new());
        let fermee = Arc::new(BoiteAuxLettres::new());
        assert!(fermee.fermer_si_libre());
        let ports = [
            Port {
                id: IdSolution(1),
                boite: Arc::clone(&fermee),
            },
            Port {
                id: IdSolution(2),
                boite: Arc::clone(&ouverte),
            },
        ];
        let reacteur = Reacteur::new(&ctx, IdSolution(0), &regles).avec_ports(&ports);
        let mut lot = Lot::new(
            vec![
                Element::Molecule(molecule(&ids, 3)),
                Element::Molecule(molecule(&ids, 4)),
            ],
            0,
        );
        assert_eq!(reacteur.reagir(&mut lot, 0).unwrap(), 2);
        assert_eq!(fermee.nombre(), 0);
        let mut recues: Vec<i64> = ouverte
            .relever()
            .iter()
            .filter_map(|m| m.valeur.comme_entier())
            .collect();
        recues.sort_unstable();
        assert_eq!(recues, vec![3, 4]);
        // Toutes les boites fermees : la reaction n'est plus possible et le
        // jeton d'etape n'est pas consomme.
        assert!(ouverte.fermer_si_libre());
        let mut lot = Lot::new(vec![Element::Molecule(molecule(&ids, 5))], 0);
        assert_eq!(reacteur.reagir(&mut lot, 0).unwrap(), 0);
        assert_eq!(valeurs(lot), vec![5]);
    }
}
