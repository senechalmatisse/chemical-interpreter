// Types des evenements de trace (format JSON Lines, docs/format-trace.md).
// Version 2 : champ `retiree` des reactions (regles a usage unique),
// evenements `envoi` et `reception` (injection asynchrone) ; une trace de
// version 1 est une trace de version 2 valide.

export type IdSolution = string;
export type IdMolecule = number;

/** Valeur d'une molecule : entier, booleen, chaine ou n-uplet. */
export type ValeurMolecule = number | boolean | string | ValeurMolecule[];

export interface Molecule {
  readonly id: IdMolecule;
  readonly v: ValeurMolecule;
}

export interface EvDebut {
  readonly ev: 'debut';
  readonly t: number;
  readonly graine: string;
  readonly strategie: string;
  readonly workers: number;
  readonly programme: string;
}

export interface EvInit {
  readonly ev: 'init';
  readonly t: number;
  readonly sol: IdSolution;
  readonly parent: IdSolution | null;
  readonly regles: readonly string[];
  readonly mol: readonly Molecule[];
}

export interface EvReaction {
  readonly ev: 'reaction';
  readonly t: number;
  readonly n: number;
  readonly w: number;
  readonly sol: IdSolution;
  readonly regle: string;
  readonly in: readonly IdMolecule[];
  readonly in_sol: readonly IdSolution[];
  readonly out: readonly Molecule[];
  /** Regle a usage unique retiree de `sol` par cette reaction. */
  readonly retiree: boolean;
}

export interface EvDissolution {
  readonly ev: 'dissolution';
  readonly t: number;
  readonly n: number;
  readonly sol: IdSolution;
  readonly cible: IdSolution;
  readonly mol: readonly IdMolecule[];
  readonly sous: readonly IdSolution[];
}

export interface EvInjection {
  readonly ev: 'injection';
  readonly t: number;
  readonly n: number;
  readonly sol: IdSolution;
  readonly cible: IdSolution;
  readonly out: readonly Molecule[];
}

/** Injection asynchrone : molecules deposees dans la boite aux lettres de `cible`. */
export interface EvEnvoi {
  readonly ev: 'envoi';
  readonly t: number;
  readonly n: number;
  readonly sol: IdSolution;
  readonly cible: IdSolution;
  readonly out: readonly Molecule[];
}

/** Releve du courrier : les molecules en transit rejoignent `sol`. */
export interface EvReception {
  readonly ev: 'reception';
  readonly t: number;
  readonly sol: IdSolution;
  readonly mol: readonly IdMolecule[];
}

export interface EvDestruction {
  readonly ev: 'destruction';
  readonly t: number;
  readonly n: number;
  readonly sol: IdSolution;
  readonly cible: IdSolution;
}

export interface EvInertie {
  readonly ev: 'inertie';
  readonly t: number;
  readonly sol: IdSolution;
}

export interface EvFin {
  readonly ev: 'fin';
  readonly t: number;
  readonly reactions: number;
  readonly inerte: boolean;
  readonly motif: string;
}

export type Evenement =
  | EvDebut
  | EvInit
  | EvReaction
  | EvDissolution
  | EvInjection
  | EvEnvoi
  | EvReception
  | EvDestruction
  | EvInertie
  | EvFin;

/** Texte d'une valeur dans la syntaxe du langage. */
export function formaterValeur(v: ValeurMolecule): string {
  if (Array.isArray(v)) {
    return `(${v.map(formaterValeur).join(', ')})`;
  }
  if (typeof v === 'string') {
    return JSON.stringify(v);
  }
  return String(v);
}
