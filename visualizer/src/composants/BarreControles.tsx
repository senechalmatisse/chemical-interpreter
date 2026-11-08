// Controles de lecture : debut, pas arriere, lecture/pause, pas avant, fin, curseur de position et vitesse.

import type { ChangeEvent } from 'react';
import { VITESSES, type Instantane, type Lecteur } from '../lecteur/lecteur.ts';

interface Proprietes {
  readonly lecteur: Lecteur;
  readonly instantane: Instantane;
}

export function BarreControles({ lecteur, instantane }: Proprietes) {
  const { position, total, etat, vitesse } = instantane;
  const enLecture = etat === 'lecture';
  return (
    <div className="controles" role="toolbar" aria-label="Lecture de la trace">
      <div className="boutons">
        <button type="button" onClick={() => lecteur.debut()} disabled={position === 0} title="Debut (Origine)" aria-label="Aller au debut">
          {'\u23ee'}
        </button>
        <button type="button" onClick={() => lecteur.precedent()} disabled={position === 0} title="Etape precedente (fleche gauche)" aria-label="Etape precedente">
          {'\u25c0'}
        </button>
        <button
          type="button"
          className="principal"
          onClick={() => lecteur.basculer()}
          title="Lecture / pause (espace)"
          aria-label={enLecture ? 'Pause' : 'Lecture'}
        >
          {enLecture ? '\u23f8' : '\u25b6'}
        </button>
        <button type="button" onClick={() => lecteur.suivant()} disabled={position >= total} title="Etape suivante (fleche droite)" aria-label="Etape suivante">
          {'\u25b6\u25b6'}
        </button>
        <button type="button" onClick={() => lecteur.finir()} disabled={position >= total} title="Fin (Fin)" aria-label="Aller a la fin">
          {'\u23ed'}
        </button>
      </div>
      <label className="curseur">
        <span className="visuellement-cache">Position dans la trace</span>
        <input
          type="range"
          min={0}
          max={total}
          value={position}
          onChange={(e: ChangeEvent<HTMLInputElement>) => lecteur.allerA(Number(e.target.value))}
        />
      </label>
      <span className="compteur" aria-live="polite">
        etape <strong>{position}</strong> / {total}
      </span>
      <label className="vitesse">
        vitesse
        <select value={vitesse} onChange={(e: ChangeEvent<HTMLSelectElement>) => lecteur.changerVitesse(Number(e.target.value))}>
          {VITESSES.map((v) => (
            <option key={v} value={v}>
              {v} etape{v > 1 ? 's' : ''}/s
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
