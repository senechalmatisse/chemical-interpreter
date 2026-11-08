// Outils des tests (node:test) : lecture des traces de demonstration.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export interface Demonstration {
  readonly fichier: string;
  readonly titre: string;
  readonly attendu: string;
}

const repertoire = fileURLToPath(new URL('../public/exemples/', import.meta.url));

export function demonstrations(): Demonstration[] {
  return JSON.parse(readFileSync(`${repertoire}index.json`, 'utf8')) as Demonstration[];
}

export function lireTrace(fichier: string): string {
  return readFileSync(`${repertoire}${fichier}`, 'utf8');
}
