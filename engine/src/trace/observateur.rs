//! Observateur : le moteur publie des evenements sans connaitre leurs
//! consommateurs (trace JSONL, console, pas-a-pas, tests...).
//!
//! Le [`Diffuseur`] (sujet) ne fait rien quand il n'a aucun abonne : il se
//! comporte alors comme un objet nul et le moteur evite meme de construire
//! les evenements (`est_actif`), ce qui rend la trace desactivable a cout
//! quasi nul (EF-V7).

use super::Evenement;
use std::io;

/// Consommateur d'evenements.
///
/// Les implementations doivent etre sures entre threads : en mode parallele,
/// plusieurs workers publient simultanement (chaque lot reste contigu).
pub trait Observateur: Send + Sync {
    /// Recoit un evenement.
    fn notifier(&self, evenement: &Evenement);

    /// Recoit un lot d'evenements contigus (par defaut : un par un).
    fn notifier_lot(&self, evenements: &[Evenement]) {
        for e in evenements {
            self.notifier(e);
        }
    }

    /// Vrai si l'observateur doit recevoir chaque reaction sans delai
    /// (mode interactif) ; sinon les evenements peuvent etre groupes.
    fn exige_immediatete(&self) -> bool {
        false
    }

    /// Fin de l'execution : vidage des tampons, compte rendu d'erreur d'E/S.
    fn terminer(&self) -> io::Result<()> {
        Ok(())
    }
}

/// Taille des lots quand aucun observateur n'exige l'immediatete.
pub const TAILLE_LOT: usize = 512;

/// Sujet : diffuse chaque evenement a tous ses abonnes.
#[derive(Default)]
pub struct Diffuseur {
    observateurs: Vec<Box<dyn Observateur>>,
}

impl Diffuseur {
    /// Diffuseur sans abonne (comportement d'objet nul).
    pub fn new() -> Self {
        Self::default()
    }

    /// Abonne un observateur.
    pub fn abonner(&mut self, observateur: Box<dyn Observateur>) {
        self.observateurs.push(observateur);
    }

    /// Variante chainable de [`Diffuseur::abonner`].
    pub fn avec(mut self, observateur: impl Observateur + 'static) -> Self {
        self.abonner(Box::new(observateur));
        self
    }

    /// Vrai si au moins un observateur est abonne.
    pub fn est_actif(&self) -> bool {
        !self.observateurs.is_empty()
    }

    /// Taille de lot a utiliser par les producteurs d'evenements.
    pub fn taille_lot(&self) -> usize {
        if self.observateurs.iter().any(|o| o.exige_immediatete()) {
            1
        } else {
            TAILLE_LOT
        }
    }

    /// Diffuse un evenement.
    pub fn diffuser(&self, evenement: &Evenement) {
        for o in &self.observateurs {
            o.notifier(evenement);
        }
    }

    /// Diffuse un lot d'evenements.
    pub fn diffuser_lot(&self, evenements: &[Evenement]) {
        if evenements.is_empty() {
            return;
        }
        for o in &self.observateurs {
            o.notifier_lot(evenements);
        }
    }

    /// Termine tous les observateurs ; retourne la premiere erreur.
    pub fn terminer(&self) -> io::Result<()> {
        let mut premiere = Ok(());
        for o in &self.observateurs {
            if let Err(e) = o.terminer()
                && premiere.is_ok()
            {
                premiere = Err(e);
            }
        }
        premiere
    }
}

/// Observateur memorisant les evenements (tests, outils).
#[derive(Default)]
pub struct Enregistreur {
    evenements: std::sync::Mutex<Vec<Evenement>>,
}

impl Enregistreur {
    /// Enregistreur vide.
    pub fn new() -> Self {
        Self::default()
    }

    /// Copie des evenements recus.
    pub fn evenements(&self) -> Vec<Evenement> {
        self.evenements
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Observateur for Enregistreur {
    fn notifier(&self, evenement: &Evenement) {
        self.notifier_lot(std::slice::from_ref(evenement));
    }

    fn notifier_lot(&self, evenements: &[Evenement]) {
        self.evenements
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend_from_slice(evenements);
    }
}

impl<T: Observateur + ?Sized> Observateur for std::sync::Arc<T> {
    fn notifier(&self, evenement: &Evenement) {
        (**self).notifier(evenement);
    }
    fn notifier_lot(&self, evenements: &[Evenement]) {
        (**self).notifier_lot(evenements);
    }
    fn exige_immediatete(&self) -> bool {
        (**self).exige_immediatete()
    }
    fn terminer(&self) -> io::Result<()> {
        (**self).terminer()
    }
}
