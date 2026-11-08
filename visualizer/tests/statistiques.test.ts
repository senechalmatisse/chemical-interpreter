import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chargerSession } from '../src/modele/session.ts';
import { CASES_FRISE, compterJusqua } from '../src/modele/statistiques.ts';
import { lireTrace } from './outils.ts';

test('recherche dichotomique des cumuls', () => {
  const positions = [2, 5, 5, 9];
  assert.equal(compterJusqua(positions, 0), 0);
  assert.equal(compterJusqua(positions, 5), 3);
  assert.equal(compterJusqua(positions, 100), 4);
});

test('reactions par regle et par worker, frise agregee', () => {
  const { statistiques } = chargerSession('crible', lireTrace('crible_1000_w4.jsonl'));
  assert.equal(statistiques.reactions, 831);
  assert.deepEqual(statistiques.regles, ['crible']);
  assert.ok(statistiques.workers.length > 1, 'plusieurs workers dans une trace parallele');
  const parWorker = [...statistiques.positionsParWorker.values()].reduce((s, p) => s + p.length, 0);
  assert.equal(parWorker, 831);
  const cases = statistiques.frise.reduce((s, c) => s + c.reactions, 0);
  assert.equal(cases, 831);
  assert.ok(statistiques.frise.length <= CASES_FRISE * statistiques.workers.length);
  assert.equal(statistiques.fin?.motif, 'inertie');
  assert.equal(statistiques.courbe[0]?.molecules, 0);
  assert.equal(statistiques.courbe.at(-1)?.molecules, 168);
});
