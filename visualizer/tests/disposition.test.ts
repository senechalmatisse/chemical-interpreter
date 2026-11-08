import assert from 'node:assert/strict';
import { test } from 'node:test';
import { disposer, Emplacements, positionEmplacement, positionsTransit } from '../src/vue/disposition.ts';

test('une molecule garde son emplacement, un produit prend le plus petit libre', () => {
  const e = new Emplacements();
  const a = e.attribuer('s0', [1, 2, 3, 4]);
  assert.deepEqual([...a.entries()], [[1, 0], [2, 1], [3, 2], [4, 3]]);
  // reaction : 2 et 3 consommes, 5 produit
  const b = e.attribuer('s0', [1, 4, 5]);
  assert.equal(b.get(1), 0);
  assert.equal(b.get(4), 3);
  assert.equal(b.get(5), 1);
});

test('compactage quand les trous dominent', () => {
  const e = new Emplacements();
  e.attribuer('s0', Array.from({ length: 100 }, (_, i) => i));
  const c = e.attribuer('s0', [99]);
  assert.equal(c.get(99), 0);
});

test('spirale : emplacements distincts et suffisamment espaces', () => {
  const points = Array.from({ length: 200 }, (_, k) => positionEmplacement(k));
  let minimum = Infinity;
  for (let i = 0; i < points.length; i += 1) {
    for (let j = i + 1; j < points.length; j += 1) {
      minimum = Math.min(minimum, Math.hypot(points[i]!.x - points[j]!.x, points[i]!.y - points[j]!.y));
    }
  }
  // diametre des molecules : 2 x 0.82 ; on tolere un leger recouvrement visuel
  assert.ok(minimum > 1.2, `distance minimale ${minimum}`);
});

test('molecules en transit : sur le bord de leur destinataire, en anneaux', () => {
  const peu = positionsTransit(3, 10, 1);
  assert.equal(peu.length, 3);
  for (const p of peu) {
    assert.ok(Math.abs(Math.hypot(p.x, p.y) - 10) < 1e-9, 'sur le bord');
    assert.ok(p.y > 0, 'groupe centre en bas');
  }
  // le bord d'une solution de rayon 10 accueille floor(2 pi 10 / 2.3) = 27 molecules
  const beaucoup = positionsTransit(60, 10, 1);
  assert.equal(beaucoup.length, 60);
  const anneaux = new Set(beaucoup.map((p) => Math.round(Math.hypot(p.x, p.y) * 10) / 10));
  assert.ok(anneaux.size >= 2, `${anneaux.size} anneau(x)`);
  let minimum = Infinity;
  for (let i = 0; i < beaucoup.length; i += 1) {
    for (let j = i + 1; j < beaucoup.length; j += 1) {
      minimum = Math.min(minimum, Math.hypot(beaucoup[i]!.x - beaucoup[j]!.x, beaucoup[i]!.y - beaucoup[j]!.y));
    }
  }
  assert.ok(minimum > 1.9, `distance minimale ${minimum}`);
});

test('disposition : les molecules en transit sont marquees et hors du champ', () => {
  const vue = {
    id: 's0',
    regles: [],
    inerte: false,
    molecules: [],
    transit: [],
    enfants: [
      {
        id: 's1',
        regles: ['max'],
        inerte: true,
        molecules: [{ id: 1, v: 17 }],
        transit: [
          { id: 3, v: 8 },
          { id: 2, v: 50 },
        ],
        enfants: [],
      },
    ],
  };
  const { solutions, molecules } = disposer(vue, 400, 400, 20, new Emplacements());
  const s1 = solutions.find((s) => s.id === 's1')!;
  const enTransit = molecules.filter((m) => m.transit === 's1');
  assert.deepEqual(
    enTransit.map((m) => m.id),
    [2, 3],
  );
  for (const m of enTransit) {
    assert.ok(Math.abs(Math.hypot(m.x - s1.x, m.y - s1.y) - s1.r) < 1e-6, 'sur le bord de s1');
  }
  assert.equal(molecules.find((m) => m.id === 1)?.transit, null);
});
