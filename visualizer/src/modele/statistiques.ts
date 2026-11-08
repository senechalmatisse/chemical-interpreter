// Statistiques derivees de la trace : courbe du nombre de molecules,
// reactions par regle et par worker (cumulees jusqu'a une etape), frise
// temporelle par worker.

import type { EvDebut, EvFin, Evenement } from '../trace/evenements.ts';

export interface PointCourbe {
  readonly etape: number;
  readonly molecules: number;
}

export interface CaseFrise {
  readonly worker: number;
  readonly debut: number;
  readonly fin: number;
  readonly reactions: number;
}

export interface Statistiques {
  readonly debut: EvDebut | null;
  readonly fin: EvFin | null;
  readonly reactions: number;
  readonly regles: readonly string[];
  readonly workers: readonly number[];
  readonly duree: number;
  readonly courbe: readonly PointCourbe[];
  readonly frise: readonly CaseFrise[];
  /** Etapes (positions) des reactions, par regle puis par worker, triees. */
  readonly positionsParRegle: ReadonlyMap<string, readonly number[]>;
  readonly positionsParWorker: ReadonlyMap<number, readonly number[]>;
  /** Instant de chaque etape (secondes). */
  readonly instants: readonly number[];
  /** Molecules envoyees par injection asynchrone. */
  readonly envois: number;
  /** Activations de regles a usage unique consommees. */
  readonly retirees: number;
}

/** Nombre de cases de la frise (agregation pour les longues traces). */
export const CASES_FRISE = 160;
/** Points maximum de la courbe (sous-echantillonnage regulier). */
export const POINTS_COURBE = 1500;

/** Nombre d'elements de `positions` inferieurs ou egaux a `p` (recherche dichotomique). */
export function compterJusqua(positions: readonly number[], p: number): number {
  let bas = 0;
  let haut = positions.length;
  while (bas < haut) {
    const milieu = (bas + haut) >>> 1;
    if (positions[milieu]! <= p) bas = milieu + 1;
    else haut = milieu;
  }
  return bas;
}

export function calculerStatistiques(evenements: readonly Evenement[], molecules: readonly number[]): Statistiques {
  let debut: EvDebut | null = null;
  let fin: EvFin | null = null;
  const regles: string[] = [];
  const positionsParRegle = new Map<string, number[]>();
  const positionsParWorker = new Map<number, number[]>();
  const instants: number[] = [0];
  const reactionsDatees: { w: number; t: number }[] = [];
  let envois = 0;
  let retirees = 0;
  const etapes = Math.min(evenements.length, molecules.length - 1);
  for (let i = 0; i < etapes; i += 1) {
    const e = evenements[i]!;
    const position = i + 1;
    instants.push(e.t);
    if (e.ev === 'debut') debut = e;
    if (e.ev === 'fin') fin = e;
    if (e.ev === 'init') {
      for (const r of e.regles) if (!regles.includes(r)) regles.push(r);
    }
    if (e.ev === 'reaction') {
      if (!regles.includes(e.regle)) regles.push(e.regle);
      const parRegle = positionsParRegle.get(e.regle) ?? [];
      parRegle.push(position);
      positionsParRegle.set(e.regle, parRegle);
      const parWorker = positionsParWorker.get(e.w) ?? [];
      parWorker.push(position);
      positionsParWorker.set(e.w, parWorker);
      reactionsDatees.push({ w: e.w, t: e.t });
      if (e.retiree) retirees += 1;
    }
    if (e.ev === 'envoi') envois += e.out.length;
  }
  const workers = [...positionsParWorker.keys()].sort((a, b) => a - b);
  const duree = instants.reduce((m, t) => Math.max(m, t), 0);

  const pas = Math.max(1, Math.ceil(molecules.length / POINTS_COURBE));
  const courbe: PointCourbe[] = [];
  for (let etape = 0; etape < molecules.length; etape += pas) {
    courbe.push({ etape, molecules: molecules[etape]! });
  }
  const derniere = molecules.length - 1;
  if (courbe.at(-1)?.etape !== derniere) courbe.push({ etape: derniere, molecules: molecules[derniere]! });

  const frise: CaseFrise[] = [];
  if (reactionsDatees.length > 0) {
    // boucles plutot que Math.min(...t) : pas de limite de pile sur les longues traces
    const tMin = reactionsDatees.reduce((m, r) => Math.min(m, r.t), Infinity);
    const tMax = reactionsDatees.reduce((m, r) => Math.max(m, r.t), -Infinity);
    const largeur = tMax > tMin ? (tMax - tMin) / CASES_FRISE : 1;
    const comptes = new Map<string, CaseFrise>();
    for (const r of reactionsDatees) {
      const k = Math.min(CASES_FRISE - 1, Math.floor((r.t - tMin) / largeur));
      const cle = `${r.w}:${k}`;
      const existante = comptes.get(cle);
      comptes.set(cle, {
        worker: r.w,
        debut: tMin + k * largeur,
        fin: tMin + (k + 1) * largeur,
        reactions: (existante?.reactions ?? 0) + 1,
      });
    }
    frise.push(...comptes.values());
  }

  const reactions = [...positionsParRegle.values()].reduce((s, p) => s + p.length, 0);
  return {
    debut,
    fin,
    reactions,
    regles,
    workers,
    duree,
    courbe,
    frise,
    positionsParRegle,
    positionsParWorker,
    instants,
    envois,
    retirees,
  };
}
