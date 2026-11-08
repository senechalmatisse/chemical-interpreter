// Facade : du texte d'une trace a une session prete a etre visualisee.

import { analyserTrace } from '../trace/analyseur.ts';
import type { Evenement } from '../trace/evenements.ts';
import { Historique } from './historique.ts';
import { calculerStatistiques, type Statistiques } from './statistiques.ts';

export interface Session {
  readonly nom: string;
  readonly evenements: readonly Evenement[];
  readonly historique: Historique;
  readonly statistiques: Statistiques;
  /** Lignes illisibles et incoherences, a signaler a l'utilisateur. */
  readonly avertissements: readonly string[];
}

export class ErreurSession extends Error {}

export function chargerSession(nom: string, texte: string): Session {
  const { evenements, erreurs } = analyserTrace(texte);
  if (!evenements.some((e) => e.ev === 'init')) {
    const detail = erreurs[0] ? ` (ligne ${erreurs[0].ligne} : ${erreurs[0].message})` : '';
    throw new ErreurSession(`aucune solution initiale dans la trace${detail}`);
  }
  const historique = new Historique(evenements);
  const avertissements = erreurs.map((e) => `ligne ${e.ligne} ignoree : ${e.message}`);
  if (historique.incoherence) {
    const { index, message } = historique.incoherence;
    avertissements.push(`evenement ${index + 1} incoherent, lecture arretee : ${message}`);
  }
  const statistiques = calculerStatistiques(evenements, historique.molecules);
  return { nom, evenements, historique, statistiques, avertissements };
}
