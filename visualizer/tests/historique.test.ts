import assert from 'node:assert/strict';
import { test } from 'node:test';
import { formaterCanonique } from '../src/modele/canonique.ts';
import { INTERVALLE_MEMENTO } from '../src/modele/historique.ts';
import { chargerSession } from '../src/modele/session.ts';
import { demonstrations, lireTrace } from './outils.ts';

test('chaque demonstration reconstruit la solution finale du moteur', () => {
  for (const d of demonstrations()) {
    const session = chargerSession(d.fichier, lireTrace(d.fichier));
    assert.equal(session.avertissements.length, 0, d.fichier);
    session.historique.allerA(session.historique.total);
    const vue = session.historique.etat.instantane();
    assert.ok(vue, d.fichier);
    assert.equal(formaterCanonique(vue), d.attendu, d.fichier);
  }
});

test('reculer jusqu au debut ramene a l etat vide, avancer rejoue a l identique', () => {
  for (const d of demonstrations()) {
    const { historique } = chargerSession(d.fichier, lireTrace(d.fichier));
    const textes: string[] = [];
    while (historique.avancer()) {
      const vue = historique.etat.instantane();
      textes.push(vue ? formaterCanonique(vue) : '');
      assert.equal(historique.etat.nombreMolecules, historique.molecules[historique.position]);
    }
    for (let p = historique.total - 1; p >= 1; p -= 1) {
      historique.reculer();
      const vue = historique.etat.instantane();
      assert.equal(vue ? formaterCanonique(vue) : '', textes[p - 1], `${d.fichier} position ${p}`);
    }
    historique.reculer();
    assert.equal(historique.etat.instantane(), null);
    assert.equal(historique.etat.nombreMolecules, 0);
  }
});

test('l acces direct (memento) donne le meme etat que le pas-a-pas', () => {
  const d = demonstrations().find((x) => x.fichier.startsWith('crible_1000'));
  assert.ok(d);
  const session = chargerSession(d.fichier, lireTrace(d.fichier));
  const reference = chargerSession(d.fichier, lireTrace(d.fichier)).historique;
  const etats = new Map<number, string>();
  while (reference.avancer()) {
    if (reference.position % 37 === 0) etats.set(reference.position, formaterCanonique(reference.etat.instantane()!));
  }
  assert.ok(session.historique.total > 2 * INTERVALLE_MEMENTO);
  const positions = [...etats.keys()].sort(() => 0.5 - Math.random());
  for (const p of positions) {
    session.historique.allerA(p);
    assert.equal(formaterCanonique(session.historique.etat.instantane()!), etats.get(p), `position ${p}`);
  }
});

test('une trace incoherente est tronquee et signalee', () => {
  const texte = [
    '{"t":0,"ev":"init","sol":"s0","parent":null,"regles":["r"],"mol":[{"id":1,"v":1},{"id":2,"v":2}]}',
    '{"t":0,"ev":"reaction","n":1,"w":0,"sol":"s0","regle":"r","in":[1,2],"out":[{"id":3,"v":3}]}',
    '{"t":0,"ev":"reaction","n":2,"w":0,"sol":"s0","regle":"r","in":[1,3],"out":[{"id":4,"v":4}]}',
  ].join('\n');
  const session = chargerSession('falsifiee', texte);
  assert.equal(session.historique.total, 2);
  assert.match(session.avertissements[0] ?? '', /evenement 3 incoherent/);
});

const INIT = [
  '{"t":0,"ev":"init","sol":"s0","parent":null,"regles":["une","alimenter"],"mol":[{"id":1,"v":1}]}',
  '{"t":0,"ev":"init","sol":"s1","parent":"s0","regles":["max"],"mol":[{"id":2,"v":17}]}',
];

test('regle a usage unique : retiree, restauree en arriere, refusee une seconde fois', () => {
  const texte = [
    ...INIT,
    '{"t":0,"ev":"reaction","n":1,"w":0,"sol":"s0","regle":"une","in":[1],"out":[{"id":3,"v":2}],"retiree":true}',
    '{"t":0,"ev":"reaction","n":2,"w":0,"sol":"s0","regle":"une","in":[3],"out":[{"id":4,"v":3}],"retiree":true}',
  ].join('\n');
  const session = chargerSession('usage-unique', texte);
  assert.equal(session.historique.total, 3);
  assert.match(session.avertissements[0] ?? '', /evenement 4 incoherent.*plus\) active/);
  const { historique } = session;
  historique.allerA(3);
  assert.deepEqual(historique.etat.noeud('s0').regles, ['alimenter']);
  assert.equal(formaterCanonique(historique.etat.instantane()!), '{ 2, { 17 | max } | alimenter }');
  historique.reculer();
  assert.deepEqual(historique.etat.noeud('s0').regles, ['une', 'alimenter']);
  assert.equal(session.statistiques.retirees, 1);
});

test('envoi asynchrone : molecules en transit puis recues, annulables', () => {
  const texte = [
    ...INIT,
    '{"t":0,"ev":"reaction","n":1,"w":0,"sol":"s0","regle":"alimenter","in":[],"out":[],"retiree":true}',
    '{"t":0,"ev":"envoi","n":1,"sol":"s0","cible":"s1","out":[{"id":5,"v":50},{"id":6,"v":8}]}',
    '{"t":0,"ev":"reception","sol":"s1","mol":[6,5]}',
    '{"t":0,"ev":"reception","sol":"s1","mol":[5]}',
  ].join('\n');
  const session = chargerSession('asynchrone', texte);
  const { historique } = session;
  assert.equal(historique.total, 5);
  assert.match(session.avertissements[0] ?? '', /evenement 6 incoherent.*jamais envoyee/);
  historique.allerA(4);
  const vue = historique.etat.instantane()!;
  assert.deepEqual(
    vue.enfants[0]!.transit.map((m) => m.v),
    [50, 8],
  );
  // en transit : pas encore dans la sous-solution, mais comptees
  assert.equal(formaterCanonique(vue), '{ 1, { 17 | max } | une }');
  assert.equal(historique.etat.nombreMolecules, 4);
  assert.equal(historique.etat.nombreEnTransit, 2);
  historique.avancer();
  assert.equal(formaterCanonique(historique.etat.instantane()!), '{ 1, { 8, 17, 50 | max } | une }');
  assert.equal(historique.etat.nombreEnTransit, 0);
  assert.equal(historique.etat.nombreMolecules, 4);
  historique.reculer();
  assert.equal(historique.etat.nombreEnTransit, 2);
  historique.allerA(2);
  assert.equal(historique.etat.nombreEnTransit, 0);
  assert.equal(session.statistiques.envois, 2);
});

test('une sous-solution qui attend du courrier ne peut pas etre detruite', () => {
  const texte = [
    ...INIT,
    '{"t":0,"ev":"envoi","n":1,"sol":"s0","cible":"s1","out":[{"id":5,"v":50}]}',
    '{"t":0,"ev":"destruction","n":2,"sol":"s0","cible":"s1"}',
  ].join('\n');
  const session = chargerSession('courrier', texte);
  assert.equal(session.historique.total, 3);
  assert.match(session.avertissements[0] ?? '', /transit/);
});

test('une solution qui attend du courrier n est pas affichee inerte', () => {
  const texte = [
    ...INIT,
    '{"t":0,"ev":"inertie","sol":"s1"}',
    '{"t":0,"ev":"envoi","n":1,"sol":"s0","cible":"s1","out":[{"id":5,"v":50}]}',
    '{"t":0,"ev":"reception","sol":"s1","mol":[5]}',
    '{"t":0,"ev":"inertie","sol":"s1"}',
  ].join('\n');
  const { historique } = chargerSession('inertie', texte);
  const inerteS1 = () => historique.etat.instantane()!.enfants[0]!.inerte;
  historique.allerA(3);
  assert.equal(inerteS1(), true);
  historique.avancer();
  assert.equal(inerteS1(), false, 'courrier en transit');
  historique.avancer();
  assert.equal(inerteS1(), false, 'courrier recu, pas encore traite');
  historique.avancer();
  assert.equal(inerteS1(), true);
});
