// Forme canonique d'une solution, identique a celle du moteur (chem run) :
// valeurs triees (entiers < booleens < chaines < n-uplets), sous-solutions
// triees par leur texte, puis regles dans leur ordre.

import { formaterValeur, type ValeurMolecule } from '../trace/evenements.ts';
import type { VueSolution } from './etat.ts';

function rangType(v: ValeurMolecule): number {
  if (typeof v === 'number') return 0;
  if (typeof v === 'boolean') return 1;
  if (typeof v === 'string') return 2;
  return 3;
}

/** Comparaison des chaines par points de code (ordre des octets UTF-8). */
function comparerChaines(a: string, b: string): number {
  const pa = [...a];
  const pb = [...b];
  for (let i = 0; i < Math.min(pa.length, pb.length); i += 1) {
    const d = pa[i]!.codePointAt(0)! - pb[i]!.codePointAt(0)!;
    if (d !== 0) return d;
  }
  return pa.length - pb.length;
}

export function comparerValeurs(a: ValeurMolecule, b: ValeurMolecule): number {
  const d = rangType(a) - rangType(b);
  if (d !== 0) return d;
  if (typeof a === 'number' && typeof b === 'number') return a - b;
  if (typeof a === 'boolean' && typeof b === 'boolean') return Number(a) - Number(b);
  if (typeof a === 'string' && typeof b === 'string') return comparerChaines(a, b);
  if (Array.isArray(a) && Array.isArray(b)) {
    for (let i = 0; i < Math.min(a.length, b.length); i += 1) {
      const c = comparerValeurs(a[i]!, b[i]!);
      if (c !== 0) return c;
    }
    return a.length - b.length;
  }
  return 0;
}

export function formaterCanonique(vue: VueSolution): string {
  const valeurs = vue.molecules.map((m) => m.v).sort(comparerValeurs).map(formaterValeur);
  const sous = vue.enfants.map(formaterCanonique).sort(comparerChaines);
  const morceaux = [...valeurs, ...sous];
  let texte = '{';
  if (morceaux.length > 0) texte += ` ${morceaux.join(', ')}`;
  if (vue.regles.length > 0) texte += ` | ${vue.regles.join(', ')}`;
  return `${texte} }`;
}
