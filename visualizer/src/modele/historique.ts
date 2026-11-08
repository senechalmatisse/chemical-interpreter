// Historique : suite des commandes et position courante (avance, recul,
// acces direct). Des memento sont poses a intervalle regulier pour qu'un
// saut lointain ne rejoue qu'un petit nombre de commandes.

import type { Evenement } from '../trace/evenements.ts';
import { fabriquerCommande, type Commande } from './commandes.ts';
import { EtatSolutions, type MementoEtat } from './etat.ts';

/** Intervalle entre deux memento. */
export const INTERVALLE_MEMENTO = 256;

export interface Incoherence {
  readonly index: number;
  readonly message: string;
}

export class Historique {
  readonly commandes: readonly Commande[];
  readonly etat = new EtatSolutions();
  /** Nombre de molecules apres chaque etape (indice 0 : etat vide). */
  readonly molecules: readonly number[];
  /** Premiere incoherence rencontree (la trace est tronquee avant elle). */
  readonly incoherence: Incoherence | null;
  private positionCourante = 0;
  private readonly memento = new Map<number, MementoEtat>();

  constructor(evenements: readonly Evenement[]) {
    const commandes: Commande[] = [];
    const molecules = [0];
    let incoherence: Incoherence | null = null;
    this.memento.set(0, this.etat.memento());
    for (const [index, e] of evenements.entries()) {
      const commande = fabriquerCommande(e);
      try {
        commande.appliquer(this.etat);
      } catch (erreur) {
        incoherence = { index, message: (erreur as Error).message };
        break;
      }
      commandes.push(commande);
      molecules.push(this.etat.nombreMolecules);
      if (commandes.length % INTERVALLE_MEMENTO === 0) {
        this.memento.set(commandes.length, this.etat.memento());
      }
    }
    this.commandes = commandes;
    this.molecules = molecules;
    this.incoherence = incoherence;
    // Retour a l'etat initial (vide), sans rejouer d'annulations.
    this.etat.restaurer(this.memento.get(0)!);
    // L'application suivante doit repartir des commandes, pas d'un etat partiel.
    this.positionCourante = 0;
  }

  get position(): number {
    return this.positionCourante;
  }

  get total(): number {
    return this.commandes.length;
  }

  /** Commande appliquee en dernier (celle de l'etape courante). */
  get courante(): Commande | null {
    return this.positionCourante > 0 ? this.commandes[this.positionCourante - 1]! : null;
  }

  /** Commande qui sera appliquee a la prochaine avance. */
  get suivante(): Commande | null {
    return this.commandes[this.positionCourante] ?? null;
  }

  avancer(): Commande | null {
    const c = this.commandes[this.positionCourante];
    if (!c) return null;
    c.appliquer(this.etat);
    this.positionCourante += 1;
    return c;
  }

  reculer(): Commande | null {
    if (this.positionCourante === 0) return null;
    this.positionCourante -= 1;
    const c = this.commandes[this.positionCourante]!;
    c.annuler(this.etat);
    return c;
  }

  /** Va directement a une position (0 = etat initial vide). */
  allerA(cible: number): void {
    const p = Math.max(0, Math.min(this.total, Math.round(cible)));
    const point = Math.floor(p / INTERVALLE_MEMENTO) * INTERVALLE_MEMENTO;
    const memento = this.memento.get(point);
    const distanceDirecte = Math.abs(p - this.positionCourante);
    if (memento && p - point < distanceDirecte) {
      this.etat.restaurer(memento);
      this.positionCourante = point;
    }
    while (this.positionCourante < p) this.avancer();
    while (this.positionCourante > p) this.reculer();
  }
}
