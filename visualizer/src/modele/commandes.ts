// Patron Commande : chaque evenement de trace devient une commande qui sait
// s'appliquer a l'etat et s'annuler (retour arriere). Les donnees
// necessaires a l'annulation sont memorisees lors de l'application.

import type {
  EvDestruction,
  EvDissolution,
  EvEnvoi,
  EvInertie,
  EvInit,
  EvInjection,
  EvReaction,
  EvReception,
  Evenement,
  IdMolecule,
  IdSolution,
  ValeurMolecule,
} from '../trace/evenements.ts';
import { formaterValeur } from '../trace/evenements.ts';
import { copierNoeud, ErreurCoherence, type EtatSolutions, type NoeudSolution } from './etat.ts';

export interface Commande {
  readonly evenement: Evenement;
  appliquer(etat: EtatSolutions): void;
  annuler(etat: EtatSolutions): void;
  /** Description lisible (valable apres application). */
  decrire(): string;
}

const liste = (valeurs: readonly ValeurMolecule[]) => (valeurs.length === 0 ? 'rien' : valeurs.map(formaterValeur).join(', '));

/** Molecules mises en evidence par une etape (reactifs consommes, produits). */
export interface Effets {
  readonly consommees: ReadonlySet<IdMolecule>;
  readonly produites: ReadonlySet<IdMolecule>;
  readonly solutions: ReadonlySet<IdSolution>;
}

const AUCUN_EFFET: Effets = { consommees: new Set(), produites: new Set(), solutions: new Set() };

class CommandeNeutre implements Commande {
  readonly evenement: Evenement;
  constructor(evenement: Evenement) {
    this.evenement = evenement;
  }
  appliquer(): void {}
  annuler(): void {}
  decrire(): string {
    const e = this.evenement;
    if (e.ev === 'debut') return `debut : strategie ${e.strategie}, ${e.workers} worker(s), graine ${e.graine}`;
    if (e.ev === 'fin') return `fin : ${e.reactions} reaction(s), arret par ${e.motif}${e.inerte ? ' (solution inerte)' : ''}`;
    return e.ev;
  }
}

class CommandeInit implements Commande {
  readonly evenement: EvInit;
  constructor(evenement: EvInit) {
    this.evenement = evenement;
  }
  appliquer(etat: EtatSolutions): void {
    const e = this.evenement;
    etat.creerSolution(e.sol, e.parent, e.regles);
    for (const m of e.mol) etat.ajouterMolecule(e.sol, m.id, m.v);
  }
  annuler(etat: EtatSolutions): void {
    const e = this.evenement;
    for (const m of e.mol) etat.retirerMolecule(e.sol, m.id);
    etat.supprimerSolutionVide(e.sol);
  }
  decrire(): string {
    const e = this.evenement;
    const regles = e.regles.length > 0 ? ` | ${e.regles.join(', ')}` : '';
    const parent = e.parent ? ` dans ${e.parent}` : ' (solution principale)';
    return `initialisation de ${e.sol}${parent} : ${e.mol.length} molecule(s)${regles}`;
  }
}

class CommandeReaction implements Commande {
  readonly evenement: EvReaction;
  private consommees: [IdMolecule, ValeurMolecule][] = [];
  private inerteAvant = false;
  private reglesAvant: readonly string[] = [];
  constructor(evenement: EvReaction) {
    this.evenement = evenement;
  }
  appliquer(etat: EtatSolutions): void {
    const e = this.evenement;
    const noeud = etat.noeud(e.sol);
    const rang = noeud.regles.indexOf(e.regle);
    if (rang < 0) throw new ErreurCoherence(`reaction ${e.n} : la regle '${e.regle}' n'est pas (ou plus) active dans ${e.sol}`);
    for (const id of e.in) {
      if (!noeud.molecules.has(id)) throw new ErreurCoherence(`reaction ${e.n} : molecule ${id} absente de ${e.sol}`);
    }
    for (const s of e.in_sol) {
      if (etat.noeud(s).parent !== e.sol) throw new ErreurCoherence(`reaction ${e.n} : ${s} n'est pas dans ${e.sol}`);
    }
    this.consommees = e.in.map((id) => [id, etat.retirerMolecule(e.sol, id)]);
    for (const m of e.out) etat.ajouterMolecule(e.sol, m.id, m.v);
    this.inerteAvant = noeud.inerte;
    noeud.inerte = false;
    this.reglesAvant = noeud.regles;
    if (e.retiree) noeud.regles = noeud.regles.filter((_, i) => i !== rang);
  }
  annuler(etat: EtatSolutions): void {
    const e = this.evenement;
    for (const m of e.out) etat.retirerMolecule(e.sol, m.id);
    for (const [id, v] of this.consommees) etat.ajouterMolecule(e.sol, id, v);
    const noeud = etat.noeud(e.sol);
    noeud.inerte = this.inerteAvant;
    noeud.regles = this.reglesAvant;
  }
  decrire(): string {
    const e = this.evenement;
    const reactifs = [...this.consommees.map(([, v]) => formaterValeur(v)), ...e.in_sol];
    const produits = e.out.map((m) => m.v);
    const retrait = e.retiree ? ` ; regle ${e.regle} retiree (usage unique)` : '';
    return `reaction ${e.n} : ${e.regle} dans ${e.sol} (worker ${e.w}) : ${reactifs.join(', ') || 'rien'} \u2192 ${liste(produits)}${retrait}`;
  }
}

/** Injection asynchrone : les molecules partent en transit vers la cible. */
class CommandeEnvoi implements Commande {
  readonly evenement: EvEnvoi;
  constructor(evenement: EvEnvoi) {
    this.evenement = evenement;
  }
  appliquer(etat: EtatSolutions): void {
    const e = this.evenement;
    if (etat.noeud(e.cible).parent !== e.sol) throw new ErreurCoherence(`envoi : ${e.cible} n'est pas dans ${e.sol}`);
    for (const m of e.out) etat.deposer(e.cible, m.id, m.v);
  }
  annuler(etat: EtatSolutions): void {
    const e = this.evenement;
    for (const m of e.out) etat.retirerDuTransit(e.cible, m.id);
  }
  decrire(): string {
    const e = this.evenement;
    return `envoi asynchrone de ${liste(e.out.map((m) => m.v))} vers ${e.cible} (en transit)`;
  }
}

/** Releve du courrier : les molecules en transit rejoignent la solution. */
class CommandeReception implements Commande {
  readonly evenement: EvReception;
  private recues: [IdMolecule, ValeurMolecule][] = [];
  private inerteAvant = false;
  constructor(evenement: EvReception) {
    this.evenement = evenement;
  }
  appliquer(etat: EtatSolutions): void {
    const e = this.evenement;
    const noeud = etat.noeud(e.sol);
    for (const id of e.mol) {
      if (!noeud.boite.has(id)) throw new ErreurCoherence(`reception dans ${e.sol} : molecule ${id} jamais envoyee (ou deja recue)`);
    }
    this.recues = e.mol.map((id) => [id, etat.retirerDuTransit(e.sol, id)]);
    for (const [id, v] of this.recues) etat.ajouterMolecule(e.sol, id, v);
    this.inerteAvant = noeud.inerte;
    if (this.recues.length > 0) noeud.inerte = false;
  }
  annuler(etat: EtatSolutions): void {
    const e = this.evenement;
    for (const [id, v] of this.recues) {
      etat.retirerMolecule(e.sol, id);
      etat.deposer(e.sol, id, v);
    }
    etat.noeud(e.sol).inerte = this.inerteAvant;
  }
  decrire(): string {
    const e = this.evenement;
    return `reception dans ${e.sol} : ${liste(this.recues.map(([, v]) => v))}`;
  }
}

/** Une sous-solution liee n'attend jamais de courrier (le moteur la ferme avant). */
function sansCourrier(etat: EtatSolutions, cible: IdSolution, operation: string): void {
  const n = etat.noeud(cible).boite.size;
  if (n > 0) throw new ErreurCoherence(`${operation} de ${cible} alors que ${n} molecule(s) lui sont en transit`);
}

class CommandeDissolution implements Commande {
  readonly evenement: EvDissolution;
  private dissoute: NoeudSolution | null = null;
  constructor(evenement: EvDissolution) {
    this.evenement = evenement;
  }
  appliquer(etat: EtatSolutions): void {
    const e = this.evenement;
    const cible = etat.noeud(e.cible);
    if (cible.parent !== e.sol) throw new ErreurCoherence(`dissolution : ${e.cible} n'est pas dans ${e.sol}`);
    sansCourrier(etat, e.cible, 'dissolution');
    this.dissoute = copierNoeud(cible);
    for (const [id, v] of [...cible.molecules]) {
      etat.retirerMolecule(e.cible, id);
      etat.ajouterMolecule(e.sol, id, v);
    }
    for (const enfant of [...cible.enfants]) etat.rattacher(enfant, e.sol);
    etat.supprimerSolutionVide(e.cible);
  }
  annuler(etat: EtatSolutions): void {
    const e = this.evenement;
    const d = this.dissoute;
    if (!d) return;
    etat.creerSolution(d.id, e.sol, d.regles).inerte = d.inerte;
    for (const [id, v] of d.molecules) {
      etat.retirerMolecule(e.sol, id);
      etat.ajouterMolecule(d.id, id, v);
    }
    for (const enfant of d.enfants) etat.rattacher(enfant, d.id);
  }
  decrire(): string {
    const e = this.evenement;
    return `dissolution de ${e.cible} : ${e.mol.length} molecule(s) et ${e.sous.length} sous-solution(s) liberees dans ${e.sol}`;
  }
}

class CommandeInjection implements Commande {
  readonly evenement: EvInjection;
  private inerteAvant = false;
  constructor(evenement: EvInjection) {
    this.evenement = evenement;
  }
  appliquer(etat: EtatSolutions): void {
    const e = this.evenement;
    const cible = etat.noeud(e.cible);
    if (cible.parent !== e.sol) throw new ErreurCoherence(`injection : ${e.cible} n'est pas dans ${e.sol}`);
    for (const m of e.out) etat.ajouterMolecule(e.cible, m.id, m.v);
    this.inerteAvant = cible.inerte;
    cible.inerte = false;
  }
  annuler(etat: EtatSolutions): void {
    const e = this.evenement;
    for (const m of e.out) etat.retirerMolecule(e.cible, m.id);
    etat.noeud(e.cible).inerte = this.inerteAvant;
  }
  decrire(): string {
    const e = this.evenement;
    return `injection de ${liste(e.out.map((m) => m.v))} dans ${e.cible} (reactivee)`;
  }
}

class CommandeDestruction implements Commande {
  readonly evenement: EvDestruction;
  private sousArbre: NoeudSolution[] = [];
  constructor(evenement: EvDestruction) {
    this.evenement = evenement;
  }
  appliquer(etat: EtatSolutions): void {
    const e = this.evenement;
    if (etat.noeud(e.cible).parent !== e.sol) throw new ErreurCoherence(`destruction : ${e.cible} n'est pas dans ${e.sol}`);
    sansCourrier(etat, e.cible, 'destruction');
    this.sousArbre = etat.extraireSousArbre(e.cible);
  }
  annuler(etat: EtatSolutions): void {
    etat.restaurerSousArbre(this.sousArbre);
  }
  decrire(): string {
    return `destruction de ${this.evenement.cible} et de son contenu`;
  }
}

class CommandeInertie implements Commande {
  readonly evenement: EvInertie;
  private avant = false;
  constructor(evenement: EvInertie) {
    this.evenement = evenement;
  }
  appliquer(etat: EtatSolutions): void {
    const n = etat.noeud(this.evenement.sol);
    this.avant = n.inerte;
    n.inerte = true;
  }
  annuler(etat: EtatSolutions): void {
    etat.noeud(this.evenement.sol).inerte = this.avant;
  }
  decrire(): string {
    return `${this.evenement.sol} est inerte`;
  }
}

/** Fabrique : evenement -> commande. */
export function fabriquerCommande(e: Evenement): Commande {
  switch (e.ev) {
    case 'init':
      return new CommandeInit(e);
    case 'reaction':
      return new CommandeReaction(e);
    case 'dissolution':
      return new CommandeDissolution(e);
    case 'injection':
      return new CommandeInjection(e);
    case 'envoi':
      return new CommandeEnvoi(e);
    case 'reception':
      return new CommandeReception(e);
    case 'destruction':
      return new CommandeDestruction(e);
    case 'inertie':
      return new CommandeInertie(e);
    case 'debut':
    case 'fin':
      return new CommandeNeutre(e);
  }
}

/** Molecules et solutions a mettre en evidence pour un evenement. */
export function effetsDe(e: Evenement | null): Effets {
  if (!e) return AUCUN_EFFET;
  switch (e.ev) {
    case 'reaction':
      return {
        consommees: new Set(e.in),
        produites: new Set(e.out.map((m) => m.id)),
        solutions: new Set([e.sol, ...e.in_sol]),
      };
    case 'injection':
    case 'envoi':
      return { consommees: new Set(), produites: new Set(e.out.map((m) => m.id)), solutions: new Set([e.cible]) };
    case 'reception':
      return { consommees: new Set(), produites: new Set(e.mol), solutions: new Set([e.sol]) };
    case 'dissolution':
      return { consommees: new Set(), produites: new Set(e.mol), solutions: new Set([e.sol]) };
    case 'destruction':
    case 'inertie':
      return { consommees: new Set(), produites: new Set(), solutions: new Set([e.ev === 'inertie' ? e.sol : e.cible]) };
    default:
      return AUCUN_EFFET;
  }
}
