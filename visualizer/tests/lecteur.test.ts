import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Lecteur, type Horloge } from '../src/lecteur/lecteur.ts';
import { chargerSession } from '../src/modele/session.ts';
import { lireTrace } from './outils.ts';

/** Horloge manuelle : les rappels ne s'executent qu'a la demande. */
class HorlogeManuelle implements Horloge {
  private rappels: { rappel: () => void; delai: number; annule: boolean }[] = [];
  programmer(rappel: () => void, delai: number): () => void {
    const entree = { rappel, delai, annule: false };
    this.rappels.push(entree);
    return () => {
      entree.annule = true;
    };
  }
  get enAttente(): number {
    return this.rappels.filter((r) => !r.annule).length;
  }
  get dernierDelai(): number {
    return this.rappels.at(-1)?.delai ?? 0;
  }
  avancer(): boolean {
    const r = this.rappels.find((x) => !x.annule);
    if (!r) return false;
    this.rappels = this.rappels.filter((x) => x !== r);
    r.rappel();
    return true;
  }
}

function nouveauLecteur() {
  const session = chargerSession('b07', lireTrace('b07_injection.jsonl'));
  const horloge = new HorlogeManuelle();
  return { lecteur: new Lecteur(session.historique, horloge), horloge, total: session.historique.total };
}

test('lecture complete : pause -> lecture -> termine', () => {
  const { lecteur, horloge, total } = nouveauLecteur();
  const notifications: number[] = [];
  lecteur.abonner(() => notifications.push(lecteur.instantane().position));
  assert.equal(lecteur.nomEtat, 'pause');
  lecteur.lire();
  assert.equal(lecteur.nomEtat, 'lecture');
  while (horloge.avancer()) {
    /* chaque tic avance d'une etape */
  }
  assert.equal(lecteur.nomEtat, 'termine');
  assert.equal(lecteur.instantane().position, total);
  assert.equal(Math.max(...notifications), total);
});

test('pause interrompt la programmation ; la vitesse regle le delai', () => {
  const { lecteur, horloge } = nouveauLecteur();
  lecteur.changerVitesse(8);
  lecteur.lire();
  assert.equal(horloge.dernierDelai, 125);
  horloge.avancer();
  lecteur.pause();
  assert.equal(horloge.enAttente, 0);
  assert.equal(lecteur.nomEtat, 'pause');
  assert.equal(lecteur.instantane().position, 1);
});

test('pas-a-pas avant et arriere, relecture apres la fin', () => {
  const { lecteur, total } = nouveauLecteur();
  lecteur.suivant();
  lecteur.suivant();
  assert.equal(lecteur.instantane().position, 2);
  assert.equal(lecteur.instantane().sens, 'avant');
  lecteur.precedent();
  assert.equal(lecteur.instantane().position, 1);
  assert.equal(lecteur.instantane().sens, 'arriere');
  lecteur.finir();
  assert.equal(lecteur.nomEtat, 'termine');
  lecteur.lire();
  assert.equal(lecteur.nomEtat, 'lecture');
  assert.equal(lecteur.instantane().position, 0);
  assert.ok(total > 2);
  lecteur.detruire();
});

test('l instantane ne change que sur notification (useSyncExternalStore)', () => {
  const { lecteur } = nouveauLecteur();
  const a = lecteur.instantane();
  assert.equal(lecteur.instantane(), a);
  lecteur.suivant();
  assert.notEqual(lecteur.instantane(), a);
});
