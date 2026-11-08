// Etat des solutions reconstruit a partir de la trace.
//
// Modele mutable (modifie par les commandes) ; la vue recoit des instantanes
// immuables. Les memento permettent de revenir rapidement a une etape
// lointaine (patron Memento, utilise par l'historique).

import type { IdMolecule, IdSolution, ValeurMolecule } from '../trace/evenements.ts';

export interface NoeudSolution {
  readonly id: IdSolution;
  parent: IdSolution | null;
  /** Regles actives ; le tableau est remplace (jamais modifie) quand une
   * regle a usage unique est retiree, les instantanes restent valides. */
  regles: readonly string[];
  readonly molecules: Map<IdMolecule, ValeurMolecule>;
  /** Molecules envoyees a cette solution et pas encore recues (en transit). */
  readonly boite: Map<IdMolecule, ValeurMolecule>;
  readonly enfants: Set<IdSolution>;
  inerte: boolean;
}

export interface MoleculeVue {
  readonly id: IdMolecule;
  readonly v: ValeurMolecule;
}

/** Vue immuable d'une solution, pour l'affichage. */
export interface VueSolution {
  readonly id: IdSolution;
  readonly regles: readonly string[];
  readonly inerte: boolean;
  readonly molecules: readonly MoleculeVue[];
  /** Molecules en transit vers cette solution (injection asynchrone). */
  readonly transit: readonly MoleculeVue[];
  readonly enfants: readonly VueSolution[];
}

/** Copie complete de l'etat (patron Memento). */
export interface MementoEtat {
  readonly racine: IdSolution | null;
  readonly noeuds: readonly NoeudSolution[];
  readonly molecules: number;
}

export class ErreurCoherence extends Error {}

export class EtatSolutions {
  readonly solutions = new Map<IdSolution, NoeudSolution>();
  racine: IdSolution | null = null;
  private compteMolecules = 0;

  /** Molecules existantes, en transit comprises. */
  get nombreMolecules(): number {
    return this.compteMolecules;
  }

  /** Molecules envoyees et pas encore recues. */
  get nombreEnTransit(): number {
    let n = 0;
    for (const s of this.solutions.values()) n += s.boite.size;
    return n;
  }

  noeud(id: IdSolution): NoeudSolution {
    const n = this.solutions.get(id);
    if (!n) throw new ErreurCoherence(`solution ${id} inconnue`);
    return n;
  }

  ajouterMolecule(sol: IdSolution, id: IdMolecule, v: ValeurMolecule): void {
    const n = this.noeud(sol);
    if (n.molecules.has(id)) throw new ErreurCoherence(`molecule ${id} deja presente dans ${sol}`);
    n.molecules.set(id, v);
    this.compteMolecules += 1;
  }

  retirerMolecule(sol: IdSolution, id: IdMolecule): ValeurMolecule {
    const n = this.noeud(sol);
    const v = n.molecules.get(id);
    if (v === undefined) throw new ErreurCoherence(`molecule ${id} absente de ${sol}`);
    n.molecules.delete(id);
    this.compteMolecules -= 1;
    return v;
  }

  /** Depose une molecule en transit vers `sol` (elle compte dans le total). */
  deposer(sol: IdSolution, id: IdMolecule, v: ValeurMolecule): void {
    const n = this.noeud(sol);
    if (n.boite.has(id) || n.molecules.has(id)) throw new ErreurCoherence(`molecule ${id} deja presente dans ${sol}`);
    n.boite.set(id, v);
    this.compteMolecules += 1;
  }

  /** Retire une molecule en transit vers `sol`. */
  retirerDuTransit(sol: IdSolution, id: IdMolecule): ValeurMolecule {
    const n = this.noeud(sol);
    const v = n.boite.get(id);
    if (v === undefined) throw new ErreurCoherence(`molecule ${id} jamais envoyee a ${sol} (ou deja recue)`);
    n.boite.delete(id);
    this.compteMolecules -= 1;
    return v;
  }

  creerSolution(id: IdSolution, parent: IdSolution | null, regles: readonly string[]): NoeudSolution {
    if (this.solutions.has(id)) throw new ErreurCoherence(`solution ${id} creee deux fois`);
    const noeud: NoeudSolution = {
      id,
      parent,
      regles,
      molecules: new Map(),
      boite: new Map(),
      enfants: new Set(),
      inerte: false,
    };
    if (parent === null) {
      this.racine = id;
    } else {
      this.noeud(parent).enfants.add(id);
    }
    this.solutions.set(id, noeud);
    return noeud;
  }

  /** Retire une solution vide de toute molecule et de tout enfant. */
  supprimerSolutionVide(id: IdSolution): void {
    const n = this.noeud(id);
    if (n.molecules.size > 0 || n.enfants.size > 0 || n.boite.size > 0) {
      throw new ErreurCoherence(`solution ${id} non vide`);
    }
    if (n.parent !== null) this.noeud(n.parent).enfants.delete(id);
    if (this.racine === id) this.racine = null;
    this.solutions.delete(id);
  }

  /** Deplace une sous-solution sous un autre parent. */
  rattacher(id: IdSolution, parent: IdSolution): void {
    const n = this.noeud(id);
    if (n.parent !== null) this.noeud(n.parent).enfants.delete(id);
    n.parent = parent;
    this.noeud(parent).enfants.add(id);
  }

  /** Detache et retourne un sous-arbre complet (pour la destruction). */
  extraireSousArbre(id: IdSolution): NoeudSolution[] {
    const racine = this.noeud(id);
    const ordre: NoeudSolution[] = [];
    const pile = [racine];
    while (pile.length > 0) {
      const n = pile.pop()!;
      ordre.push(n);
      for (const e of n.enfants) pile.push(this.noeud(e));
    }
    if (racine.parent !== null) this.noeud(racine.parent).enfants.delete(id);
    for (const n of ordre) {
      this.compteMolecules -= n.molecules.size + n.boite.size;
      this.solutions.delete(n.id);
    }
    return ordre;
  }

  /** Reinsere un sous-arbre extrait par `extraireSousArbre`. */
  restaurerSousArbre(noeuds: readonly NoeudSolution[]): void {
    for (const n of noeuds) {
      this.solutions.set(n.id, n);
      this.compteMolecules += n.molecules.size + n.boite.size;
    }
    const racine = noeuds[0];
    if (racine && racine.parent !== null) this.noeud(racine.parent).enfants.add(racine.id);
  }

  memento(): MementoEtat {
    const noeuds = [...this.solutions.values()].map(copierNoeud);
    return { racine: this.racine, noeuds, molecules: this.compteMolecules };
  }

  restaurer(m: MementoEtat): void {
    this.solutions.clear();
    for (const n of m.noeuds) {
      this.solutions.set(n.id, copierNoeud(n));
    }
    this.racine = m.racine;
    this.compteMolecules = m.molecules;
  }

  instantane(id: IdSolution | null = this.racine): VueSolution | null {
    if (id === null) return null;
    const n = this.solutions.get(id);
    if (!n) return null;
    const enfants: VueSolution[] = [];
    for (const e of [...n.enfants].sort(comparerIds)) {
      const vue = this.instantane(e);
      if (vue) enfants.push(vue);
    }
    return {
      id: n.id,
      regles: n.regles,
      // du courrier en attente : la solution va reagir, elle n'est pas inerte
      inerte: n.inerte && n.boite.size === 0,
      molecules: [...n.molecules].map(([mid, v]) => ({ id: mid, v })),
      transit: [...n.boite].map(([mid, v]) => ({ id: mid, v })),
      enfants,
    };
  }
}

/** Copie d'un noeud dont les collections ne sont pas partagees. */
export function copierNoeud(n: NoeudSolution): NoeudSolution {
  return { ...n, molecules: new Map(n.molecules), boite: new Map(n.boite), enfants: new Set(n.enfants) };
}

export function comparerIds(a: IdSolution, b: IdSolution): number {
  return Number(a.slice(1)) - Number(b.slice(1));
}
