// Chargement d'une trace : fichier local, glisser-deposer, demonstrations
// fournies (public/exemples) ou parametre d'URL ?trace= (commande chem viz).

import { useEffect, useRef, useState, type ChangeEvent } from 'react';

export interface Demonstration {
  readonly fichier: string;
  readonly titre: string;
}

interface Proprietes {
  readonly onTexte: (nom: string, texte: string) => void;
  readonly onErreur: (message: string) => void;
}

export async function chargerUrl(url: string): Promise<string> {
  const reponse = await fetch(url);
  if (!reponse.ok) throw new Error(`${url} : ${reponse.status} ${reponse.statusText}`);
  return reponse.text();
}

export function lireFichier(fichier: File, onTexte: Proprietes['onTexte'], onErreur: Proprietes['onErreur']): void {
  fichier.text().then(
    (texte) => onTexte(fichier.name, texte),
    (e: unknown) => onErreur(`lecture de ${fichier.name} impossible : ${String(e)}`),
  );
}

export function ChargeurTrace({ onTexte, onErreur }: Proprietes) {
  const refFichier = useRef<HTMLInputElement | null>(null);
  const [demonstrations, setDemonstrations] = useState<readonly Demonstration[]>([]);

  useEffect(() => {
    chargerUrl('exemples/index.json').then(
      (texte) => setDemonstrations(JSON.parse(texte) as Demonstration[]),
      () => setDemonstrations([]),
    );
  }, []);

  const choisirDemonstration = (e: ChangeEvent<HTMLSelectElement>) => {
    const fichier = e.target.value;
    if (!fichier) return;
    chargerUrl(`exemples/${fichier}`).then(
      (texte) => onTexte(fichier, texte),
      (erreur: unknown) => onErreur(String(erreur)),
    );
    e.target.value = '';
  };

  return (
    <div className="chargeur">
      <button type="button" onClick={() => refFichier.current?.click()}>
        Ouvrir une trace{'\u2026'}
      </button>
      <input
        ref={refFichier}
        type="file"
        accept=".jsonl,.json,.ndjson,.txt"
        hidden
        onChange={(e: ChangeEvent<HTMLInputElement>) => {
          const fichier = e.target.files?.[0];
          if (fichier) lireFichier(fichier, onTexte, onErreur);
          e.target.value = '';
        }}
      />
      {demonstrations.length > 0 && (
        <select aria-label="Demonstrations" defaultValue="" onChange={choisirDemonstration}>
          <option value="" disabled>
            Demonstrations{'\u2026'}
          </option>
          {demonstrations.map((d) => (
            <option key={d.fichier} value={d.fichier}>
              {d.titre}
            </option>
          ))}
        </select>
      )}
    </div>
  );
}

export function Accueil() {
  return (
    <section className="accueil">
      <h2>Aucune trace chargee</h2>
      <p>Deposez un fichier de trace JSON Lines ici, ouvrez-en un, ou choisissez une demonstration.</p>
      <p className="code">chem run exemples/b06_dissolution.chem --workers 4 --trace-jsonl b06.jsonl</p>
      <p>
        Puis <span className="code">chem viz b06.jsonl</span> ouvre directement ce visualiseur sur la trace.
      </p>
    </section>
  );
}
