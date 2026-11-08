//! Jeton d'annulation partage entre l'interface (pas-a-pas, signal) et le
//! moteur : une demande d'arret est prise en compte au prochain point de
//! controle des workers.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Jeton d'annulation clonable.
#[derive(Debug, Clone, Default)]
pub struct JetonAnnulation {
    demande: Arc<AtomicBool>,
}

impl JetonAnnulation {
    /// Nouveau jeton (non demande).
    pub fn new() -> Self {
        Self::default()
    }

    /// Demande l'arret.
    pub fn demander(&self) {
        self.demande.store(true, Ordering::Release);
    }

    /// Vrai si l'arret a ete demande.
    pub fn est_demande(&self) -> bool {
        self.demande.load(Ordering::Acquire)
    }
}
