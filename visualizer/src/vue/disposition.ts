// Disposition stable des solutions imbriquees.
//
// Dans chaque solution, les molecules occupent des emplacements numerotes
// disposes en spirale de Vogel (tournesol) ; une molecule garde son
// emplacement tant qu'elle existe et un produit prend le plus petit
// emplacement libre (souvent celui d'un reactif qui vient de disparaitre).
// Le champ de molecules et les sous-solutions sont ensuite assembles par
// d3.packSiblings : seule la partie qui change bouge a l'ecran.
//
// Les molecules en transit (injection asynchrone) attendent sur le bord
// inferieur de leur solution destinataire, en anneaux successifs : a leur
// reception, elles rejoignent le champ (meme identifiant, deplacement anime).

import * as d3 from 'd3';
import type { IdMolecule, IdSolution, ValeurMolecule } from '../trace/evenements.ts';
import type { VueSolution } from '../modele/etat.ts';

export interface DisqueMolecule {
  readonly id: IdMolecule;
  readonly v: ValeurMolecule;
  readonly x: number;
  readonly y: number;
  readonly r: number;
  /** Solution destinataire si la molecule est en transit, sinon null. */
  readonly transit: IdSolution | null;
}

export interface DisqueSolution {
  readonly id: IdSolution;
  readonly vue: VueSolution;
  readonly x: number;
  readonly y: number;
  readonly r: number;
  readonly profondeur: number;
}

export interface Disposition {
  readonly solutions: readonly DisqueSolution[];
  readonly molecules: readonly DisqueMolecule[];
}

/** Rayon d'une molecule (unite : pas de la spirale). */
const RAYON_MOLECULE = 0.82;
/** Marge interieure d'une solution (place de l'etiquette comprise). */
const MARGE = 1.5;
/** Rayon d'une solution vide. */
const RAYON_VIDE = 1.6;
const ANGLE_OR = Math.PI * (3 - Math.sqrt(5));

/** Position d'un emplacement dans la spirale (unites). */
export function positionEmplacement(k: number): { x: number; y: number } {
  const rho = Math.sqrt(k + 0.5);
  const theta = k * ANGLE_OR;
  return { x: rho * Math.cos(theta), y: rho * Math.sin(theta) };
}

/** Rapport du rayon d'une molecule en transit a celui d'une molecule. */
const ECHELLE_TRANSIT = 0.85;

/**
 * Positions (relatives au centre de la solution) de `nombre` molecules en
 * transit de rayon `r` sur le bord d'une solution de rayon `rayon` : un
 * groupe centre en bas, puis des anneaux exterieurs quand le bord est plein.
 */
export function positionsTransit(nombre: number, rayon: number, r: number): { x: number; y: number }[] {
  const pas = 2.3 * r;
  const positions: { x: number; y: number }[] = [];
  let anneau = 0;
  while (positions.length < nombre) {
    const rho = rayon + anneau * pas;
    const capacite = Math.max(1, Math.floor((2 * Math.PI * rho) / pas));
    const k = Math.min(capacite, nombre - positions.length);
    const ecart = k === capacite ? (2 * Math.PI) / k : pas / rho;
    for (let j = 0; j < k; j += 1) {
      const angle = Math.PI / 2 + (j - (k - 1) / 2) * ecart;
      positions.push({ x: rho * Math.cos(angle), y: rho * Math.sin(angle) });
    }
    anneau += 1;
  }
  return positions;
}

/** Memoire des emplacements de molecules, par solution, entre deux rendus. */
export class Emplacements {
  private readonly parSolution = new Map<IdSolution, Map<IdMolecule, number>>();

  attribuer(sol: IdSolution, ids: readonly IdMolecule[]): ReadonlyMap<IdMolecule, number> {
    const carte = this.parSolution.get(sol) ?? new Map<IdMolecule, number>();
    const presents = new Set(ids);
    for (const id of [...carte.keys()]) if (!presents.has(id)) carte.delete(id);
    const occupes = new Set(carte.values());
    let libre = 0;
    for (const id of [...ids].sort((a, b) => a - b)) {
      if (carte.has(id)) continue;
      while (occupes.has(libre)) libre += 1;
      carte.set(id, libre);
      occupes.add(libre);
    }
    let plusGrand = -1;
    for (const k of carte.values()) plusGrand = Math.max(plusGrand, k);
    if (plusGrand + 1 > 2 * carte.size + 16) {
      // trop de trous : renumerotation compacte en conservant l'ordre
      const ordre = [...carte].sort((a, b) => a[1] - b[1]);
      ordre.forEach(([id], k) => carte.set(id, k));
    }
    this.parSolution.set(sol, carte);
    return carte;
  }

  /** Oublie les solutions disparues. */
  conserverSeulement(presentes: ReadonlySet<IdSolution>): void {
    for (const id of [...this.parSolution.keys()]) if (!presentes.has(id)) this.parSolution.delete(id);
  }
}

interface Locale {
  readonly vue: VueSolution;
  readonly r: number;
  /** Rayon d'encombrement (bord et molecules en transit), pour l'assemblage. */
  readonly enveloppe: number;
  /** Centre du champ de molecules (relatif au centre de la solution). */
  readonly champ: { x: number; y: number } | null;
  readonly emplacements: ReadonlyMap<IdMolecule, number>;
  readonly enfants: readonly { readonly locale: Locale; readonly x: number; readonly y: number }[];
}

/** Rayon d'encombrement d'une solution de rayon `r` (unites). */
function encombrement(vue: VueSolution, r: number): number {
  if (vue.transit.length === 0) return r;
  const rTransit = RAYON_MOLECULE * ECHELLE_TRANSIT;
  const loin = positionsTransit(vue.transit.length, r, rTransit).reduce((m, p) => Math.max(m, Math.hypot(p.x, p.y)), r);
  return loin + rTransit;
}

function disposerLocale(vue: VueSolution, emplacements: Emplacements): Locale {
  const slots = emplacements.attribuer(
    vue.id,
    vue.molecules.map((m) => m.id),
  );
  let nombre = 0;
  for (const k of slots.values()) nombre = Math.max(nombre, k + 1);
  const enfants = vue.enfants.map((e) => disposerLocale(e, emplacements));
  type Disque = { r: number; x: number; y: number; rang: number };
  const disques: Disque[] = [];
  if (nombre > 0) disques.push({ r: Math.sqrt(nombre) + RAYON_MOLECULE, x: 0, y: 0, rang: -1 });
  enfants.forEach((e, i) => disques.push({ r: e.enveloppe, x: 0, y: 0, rang: i }));
  if (disques.length === 0) {
    return { vue, r: RAYON_VIDE, enveloppe: encombrement(vue, RAYON_VIDE), champ: null, emplacements: slots, enfants: [] };
  }
  d3.packSiblings(disques);
  const enveloppe = d3.packEnclose(disques) ?? { x: 0, y: 0, r: disques[0]!.r };
  const champ = disques.find((d) => d.rang === -1);
  const r = enveloppe.r + MARGE;
  return {
    vue,
    r,
    enveloppe: encombrement(vue, r),
    champ: champ ? { x: champ.x - enveloppe.x, y: champ.y - enveloppe.y } : null,
    emplacements: slots,
    enfants: disques
      .filter((d) => d.rang >= 0)
      .map((d) => ({ locale: enfants[d.rang]!, x: d.x - enveloppe.x, y: d.y - enveloppe.y })),
  };
}

/** Disposition absolue (pixels) de l'arbre dans un rectangle. */
export function disposer(
  vue: VueSolution,
  largeur: number,
  hauteur: number,
  decalageHaut: number,
  emplacements: Emplacements,
): Disposition {
  const racine = disposerLocale(vue, emplacements);
  const disponible = Math.max(1, Math.min(largeur, hauteur - decalageHaut));
  const echelle = (disponible / (2 * racine.r)) * 0.98;
  const solutions: DisqueSolution[] = [];
  const molecules: DisqueMolecule[] = [];
  const presentes = new Set<IdSolution>();
  const parcourir = (locale: Locale, cx: number, cy: number, profondeur: number) => {
    presentes.add(locale.vue.id);
    solutions.push({ id: locale.vue.id, vue: locale.vue, x: cx, y: cy, r: locale.r * echelle, profondeur });
    if (locale.champ) {
      const fx = cx + locale.champ.x * echelle;
      const fy = cy + locale.champ.y * echelle;
      for (const m of locale.vue.molecules) {
        const p = positionEmplacement(locale.emplacements.get(m.id) ?? 0);
        molecules.push({ id: m.id, v: m.v, x: fx + p.x * echelle, y: fy + p.y * echelle, r: RAYON_MOLECULE * echelle, transit: null });
      }
    }
    if (locale.vue.transit.length > 0) {
      const r = RAYON_MOLECULE * ECHELLE_TRANSIT * echelle;
      const enAttente = [...locale.vue.transit].sort((a, b) => a.id - b.id);
      const positions = positionsTransit(enAttente.length, locale.r * echelle, r);
      enAttente.forEach((m, i) => {
        const p = positions[i]!;
        molecules.push({ id: m.id, v: m.v, x: cx + p.x, y: cy + p.y, r, transit: locale.vue.id });
      });
    }
    for (const e of locale.enfants) parcourir(e.locale, cx + e.x * echelle, cy + e.y * echelle, profondeur + 1);
  };
  parcourir(racine, largeur / 2, decalageHaut + (hauteur - decalageHaut) / 2, 0);
  emplacements.conserverSeulement(presentes);
  return { solutions, molecules };
}
