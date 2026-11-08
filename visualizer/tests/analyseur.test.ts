import assert from 'node:assert/strict';
import { test } from 'node:test';
import { analyserTrace } from '../src/trace/analyseur.ts';
import { formaterValeur } from '../src/trace/evenements.ts';

test('les evenements valides sont types, les lignes invalides signalees', () => {
  const texte = [
    '{"t":0,"ev":"debut","graine":"7","strategie":"aleatoire","workers":2,"programme":"p.chem"}',
    '{"t":0.001,"ev":"init","sol":"s0","parent":null,"regles":["max"],"mol":[{"id":1,"v":4},{"id":2,"v":[1,"a",true]}]}',
    '',
    'pas du json',
    '{"t":0.002,"ev":"reaction","n":1,"w":1,"sol":"s0","regle":"max","in":[1],"out":[]}',
    '{"ev":"inconnu"}',
    '{"t":0.003,"ev":"reaction","n":2,"sol":"x","regle":"max","in":[],"out":[]}',
    '{"t":0.004,"ev":"fin","reactions":1,"inerte":true,"motif":"inertie"}',
  ].join('\n');
  const { evenements, erreurs } = analyserTrace(texte);
  assert.deepEqual(
    evenements.map((e) => e.ev),
    ['debut', 'init', 'reaction', 'fin'],
  );
  assert.deepEqual(
    erreurs.map((e) => e.ligne),
    [4, 6, 7],
  );
  const reaction = evenements[2];
  assert.ok(reaction?.ev === 'reaction');
  assert.deepEqual(reaction.in_sol, []);
  assert.equal(reaction.w, 1);
});

test('formatage des valeurs dans la syntaxe du langage', () => {
  assert.equal(formaterValeur([0, [1, 'a"b'], false]), '(0, (1, "a\\"b"), false)');
  assert.equal(formaterValeur(-3), '-3');
});

test('format version 2 : regles retirees, envois et receptions', () => {
  const texte = [
    '{"t":0,"ev":"reaction","n":1,"w":0,"sol":"s0","regle":"amorcer","in":[],"out":[],"retiree":true}',
    '{"t":0,"ev":"reaction","n":2,"w":0,"sol":"s0","regle":"max","in":[1,2],"out":[]}',
    '{"t":0,"ev":"envoi","n":1,"sol":"s0","cible":"s1","out":[{"id":4,"v":50},{"id":5,"v":8}]}',
    '{"t":0,"ev":"reception","sol":"s1","mol":[4,5]}',
    '{"t":0,"ev":"reaction","n":3,"w":0,"sol":"s0","regle":"r","in":[],"out":[],"retiree":"oui"}',
    '{"t":0,"ev":"envoi","n":1,"sol":"s0","out":[]}',
    '{"t":0,"ev":"reception","sol":"s1","mol":["4"]}',
  ].join('\n');
  const { evenements, erreurs } = analyserTrace(texte);
  assert.deepEqual(
    evenements.map((e) => e.ev),
    ['reaction', 'reaction', 'envoi', 'reception'],
  );
  assert.deepEqual(
    erreurs.map((e) => e.ligne),
    [5, 6, 7],
  );
  const [retiree, ordinaire, envoi, reception] = evenements;
  assert.ok(retiree?.ev === 'reaction' && retiree.retiree);
  assert.ok(ordinaire?.ev === 'reaction' && !ordinaire.retiree, 'une trace de version 1 reste lisible');
  assert.ok(envoi?.ev === 'envoi');
  assert.equal(envoi.cible, 's1');
  assert.deepEqual(
    envoi.out.map((m) => m.v),
    [50, 8],
  );
  assert.ok(reception?.ev === 'reception');
  assert.deepEqual(reception.mol, [4, 5]);
});
