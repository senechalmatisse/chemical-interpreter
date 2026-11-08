// Lecteur de trace : lecture, pause, pas-a-pas, retour arriere, vitesse.
//
// Patron Etat : le comportement des commandes (lire, pause, tic) depend de
// l'etat courant (EnPause, EnLecture, Termine), chaque etat decidant des
// transitions. Patron Observateur : la vue s'abonne et recoit un instantane
// immuable a chaque changement (compatible useSyncExternalStore).

import type { Commande } from '../modele/commandes.ts';
import type { Historique } from '../modele/historique.ts';

/** Source de temps injectable (tests deterministes). */
export interface Horloge {
  programmer(rappel: () => void, delaiMs: number): () => void;
}

export const horlogeNavigateur: Horloge = {
  programmer(rappel, delaiMs) {
    const id = setTimeout(rappel, delaiMs);
    return () => clearTimeout(id);
  },
};

export type NomEtat = 'pause' | 'lecture' | 'termine';

export interface Instantane {
  readonly etat: NomEtat;
  readonly position: number;
  readonly total: number;
  readonly vitesse: number;
  readonly courante: Commande | null;
  /** Sens du dernier mouvement (animation des sorties et entrees). */
  readonly sens: 'avant' | 'arriere' | 'saut';
  readonly version: number;
}

interface EtatLecteur {
  readonly nom: NomEtat;
  lire(l: Lecteur): void;
  pause(l: Lecteur): void;
  tic(l: Lecteur): void;
}

const EnPause: EtatLecteur = {
  nom: 'pause',
  lire(l) {
    l.changerEtat(EnLecture);
    l.programmer();
  },
  pause() {},
  tic() {},
};

const EnLecture: EtatLecteur = {
  nom: 'lecture',
  lire() {},
  pause(l) {
    l.annulerProgrammation();
    l.changerEtat(EnPause);
  },
  tic(l) {
    if (l.avancerUneEtape()) {
      l.programmer();
    } else {
      l.changerEtat(Termine);
    }
  },
};

const Termine: EtatLecteur = {
  nom: 'termine',
  lire(l) {
    // relire depuis le debut
    l.allerA(0);
    l.changerEtat(EnLecture);
    l.programmer();
  },
  pause() {},
  tic() {},
};

export const VITESSES = [0.5, 1, 2, 4, 8, 16, 32, 64, 128, 256] as const;

export class Lecteur {
  private readonly historique: Historique;
  private readonly horloge: Horloge;
  private etat: EtatLecteur = EnPause;
  private abonnes = new Set<() => void>();
  private annulation: (() => void) | null = null;
  private vitesseCourante = 4;
  private sens: Instantane['sens'] = 'saut';
  private version = 0;
  private cache: Instantane;

  constructor(historique: Historique, horloge: Horloge = horlogeNavigateur) {
    this.historique = historique;
    this.horloge = horloge;
    this.cache = this.construireInstantane();
  }

  // ----- Observateur -----

  abonner = (rappel: () => void): (() => void) => {
    this.abonnes.add(rappel);
    return () => this.abonnes.delete(rappel);
  };

  instantane = (): Instantane => this.cache;

  private construireInstantane(): Instantane {
    return {
      etat: this.etat.nom,
      position: this.historique.position,
      total: this.historique.total,
      vitesse: this.vitesseCourante,
      courante: this.historique.courante,
      sens: this.sens,
      version: this.version,
    };
  }

  private notifier(): void {
    this.version += 1;
    this.cache = this.construireInstantane();
    for (const a of this.abonnes) a();
  }

  // ----- usage interne par les etats -----

  changerEtat(etat: EtatLecteur): void {
    this.etat = etat;
    this.notifier();
  }

  programmer(): void {
    this.annulerProgrammation();
    this.annulation = this.horloge.programmer(() => {
      this.annulation = null;
      this.etat.tic(this);
    }, 1000 / this.vitesseCourante);
  }

  annulerProgrammation(): void {
    this.annulation?.();
    this.annulation = null;
  }

  avancerUneEtape(): boolean {
    if (!this.historique.avancer()) return false;
    this.sens = 'avant';
    this.notifier();
    return true;
  }

  // ----- commandes utilisateur -----

  get nomEtat(): NomEtat {
    return this.etat.nom;
  }

  lire(): void {
    this.etat.lire(this);
  }

  pause(): void {
    this.etat.pause(this);
  }

  basculer(): void {
    if (this.etat.nom === 'lecture') this.pause();
    else this.lire();
  }

  suivant(): void {
    this.pause();
    if (!this.avancerUneEtape()) this.changerEtat(Termine);
  }

  precedent(): void {
    this.pause();
    if (this.historique.reculer()) {
      this.sens = 'arriere';
      this.etat = EnPause;
      this.notifier();
    }
  }

  allerA(position: number): void {
    const enLecture = this.etat.nom === 'lecture';
    this.historique.allerA(position);
    this.sens = 'saut';
    if (!enLecture) {
      this.etat = this.historique.position >= this.historique.total ? Termine : EnPause;
    }
    this.notifier();
  }

  debut(): void {
    this.pause();
    this.allerA(0);
  }

  finir(): void {
    this.pause();
    this.allerA(this.historique.total);
  }

  changerVitesse(vitesse: number): void {
    this.vitesseCourante = Math.max(0.1, vitesse);
    if (this.etat.nom === 'lecture') this.programmer();
    this.notifier();
  }

  /** Arrete toute programmation (demontage de la vue). */
  detruire(): void {
    this.annulerProgrammation();
    this.abonnes.clear();
  }
}
