// Analyse d'une trace JSON Lines : une ligne invalide est signalee avec son
// numero et ignoree ; le reste de la trace reste exploitable.

import type { Evenement, IdMolecule, IdSolution, Molecule, ValeurMolecule } from './evenements.ts';

export interface ErreurTrace {
  readonly ligne: number;
  readonly message: string;
}

export interface ResultatAnalyse {
  readonly evenements: Evenement[];
  readonly erreurs: ErreurTrace[];
}

class ErreurChamp extends Error {}

type Brut = Record<string, unknown>;

function champ(o: Brut, nom: string): unknown {
  if (!(nom in o)) {
    throw new ErreurChamp(`champ '${nom}' manquant`);
  }
  return o[nom];
}

function nombre(o: Brut, nom: string, defaut?: number): number {
  const v = nom in o ? o[nom] : defaut;
  if (typeof v !== 'number' || !Number.isFinite(v)) {
    throw new ErreurChamp(`champ '${nom}' : nombre attendu`);
  }
  return v;
}

function chaine(o: Brut, nom: string, defaut?: string): string {
  const v = nom in o ? o[nom] : defaut;
  if (typeof v !== 'string') {
    throw new ErreurChamp(`champ '${nom}' : chaine attendue`);
  }
  return v;
}

function solution(o: Brut, nom: string): IdSolution {
  const v = chaine(o, nom);
  if (!/^s\d+$/.test(v)) {
    throw new ErreurChamp(`champ '${nom}' : identifiant de solution 'sN' attendu`);
  }
  return v;
}

function booleen(o: Brut, nom: string, defaut: boolean): boolean {
  const v = nom in o ? o[nom] : defaut;
  if (typeof v !== 'boolean') {
    throw new ErreurChamp(`champ '${nom}' : booleen attendu`);
  }
  return v;
}

function tableau(o: Brut, nom: string): unknown[] {
  const v = nom in o ? o[nom] : [];
  if (!Array.isArray(v)) {
    throw new ErreurChamp(`champ '${nom}' : tableau attendu`);
  }
  return v;
}

function estValeur(v: unknown): v is ValeurMolecule {
  if (typeof v === 'number') return Number.isInteger(v);
  if (typeof v === 'boolean' || typeof v === 'string') return true;
  return Array.isArray(v) && v.every(estValeur);
}

function molecules(o: Brut, nom: string): Molecule[] {
  return tableau(o, nom).map((m) => {
    if (typeof m !== 'object' || m === null) {
      throw new ErreurChamp(`champ '${nom}' : molecule {id, v} attendue`);
    }
    const brut = m as Brut;
    const id = nombre(brut, 'id');
    const v = champ(brut, 'v');
    if (!estValeur(v)) {
      throw new ErreurChamp(`champ '${nom}' : valeur de molecule invalide`);
    }
    return { id, v };
  });
}

function ids(o: Brut, nom: string): IdMolecule[] {
  return tableau(o, nom).map((x) => {
    if (typeof x !== 'number') throw new ErreurChamp(`champ '${nom}' : identifiants numeriques attendus`);
    return x;
  });
}

function idsSolutions(o: Brut, nom: string): IdSolution[] {
  return tableau(o, nom).map((x) => {
    if (typeof x !== 'string' || !/^s\d+$/.test(x)) {
      throw new ErreurChamp(`champ '${nom}' : identifiants 'sN' attendus`);
    }
    return x;
  });
}

/** Convertit un objet JSON en evenement type (exception si invalide). */
export function versEvenement(o: Brut): Evenement {
  const t = nombre(o, 't', 0);
  const ev = chaine(o, 'ev');
  switch (ev) {
    case 'debut':
      return {
        ev,
        t,
        graine: String(o['graine'] ?? ''),
        strategie: chaine(o, 'strategie', ''),
        workers: nombre(o, 'workers', 1),
        programme: chaine(o, 'programme', ''),
      };
    case 'init': {
      const parent = o['parent'];
      return {
        ev,
        t,
        sol: solution(o, 'sol'),
        parent: parent === null || parent === undefined ? null : solution(o, 'parent'),
        regles: tableau(o, 'regles').map(String),
        mol: molecules(o, 'mol'),
      };
    }
    case 'reaction':
      return {
        ev,
        t,
        n: nombre(o, 'n'),
        w: nombre(o, 'w', 0),
        sol: solution(o, 'sol'),
        regle: chaine(o, 'regle'),
        in: ids(o, 'in'),
        in_sol: idsSolutions(o, 'in_sol'),
        out: molecules(o, 'out'),
        retiree: booleen(o, 'retiree', false),
      };
    case 'dissolution':
      return {
        ev,
        t,
        n: nombre(o, 'n'),
        sol: solution(o, 'sol'),
        cible: solution(o, 'cible'),
        mol: ids(o, 'mol'),
        sous: idsSolutions(o, 'sous'),
      };
    case 'injection':
      return { ev, t, n: nombre(o, 'n'), sol: solution(o, 'sol'), cible: solution(o, 'cible'), out: molecules(o, 'out') };
    case 'envoi':
      return { ev, t, n: nombre(o, 'n'), sol: solution(o, 'sol'), cible: solution(o, 'cible'), out: molecules(o, 'out') };
    case 'reception':
      return { ev, t, sol: solution(o, 'sol'), mol: ids(o, 'mol') };
    case 'destruction':
      return { ev, t, n: nombre(o, 'n'), sol: solution(o, 'sol'), cible: solution(o, 'cible') };
    case 'inertie':
      return { ev, t, sol: solution(o, 'sol') };
    case 'fin':
      return {
        ev,
        t,
        reactions: nombre(o, 'reactions', 0),
        inerte: o['inerte'] === true,
        motif: chaine(o, 'motif', ''),
      };
    default:
      throw new ErreurChamp(`evenement inconnu '${ev}'`);
  }
}

/** Analyse le texte complet d'une trace. */
export function analyserTrace(texte: string): ResultatAnalyse {
  const evenements: Evenement[] = [];
  const erreurs: ErreurTrace[] = [];
  const lignes = texte.split(/\r?\n/);
  lignes.forEach((ligne, index) => {
    if (ligne.trim() === '') return;
    try {
      const brut: unknown = JSON.parse(ligne);
      if (typeof brut !== 'object' || brut === null || Array.isArray(brut)) {
        throw new ErreurChamp('objet JSON attendu');
      }
      evenements.push(versEvenement(brut as Brut));
    } catch (e) {
      const message = e instanceof SyntaxError ? `JSON invalide (${e.message})` : (e as Error).message;
      erreurs.push({ ligne: index + 1, message });
    }
  });
  return { evenements, erreurs };
}
