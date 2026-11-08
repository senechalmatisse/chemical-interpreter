// Panneaux d'information : etape courante, parametres de l'execution,
// tableau des comptes (vue tabulaire accessible des graphiques).

import type { Commande } from '../modele/commandes.ts';
import type { Session } from '../modele/session.ts';
import { compterJusqua } from '../modele/statistiques.ts';

export function PanneauEtape({
  courante,
  suivante,
  molecules,
  transit,
}: {
  courante: Commande | null;
  suivante: Commande | null;
  molecules: number;
  transit: number;
}) {
  return (
    <section className="panneau etape" aria-label="Etape courante">
      <h2>Etape courante</h2>
      <p className="description">{courante ? courante.decrire() : 'etat initial (aucun evenement applique)'}</p>
      <p className="mesure">
        <strong>{molecules}</strong> molecule{molecules > 1 ? 's' : ''}
        {transit > 0 && <span className="transit"> dont {transit} en transit</span>}
      </p>
      {suivante && <p className="suivante">ensuite : {suivante.evenement.ev}</p>}
    </section>
  );
}

export function PanneauExecution({ session }: { session: Session }) {
  const { debut, fin, reactions, duree, workers, envois, retirees } = session.statistiques;
  return (
    <section className="panneau execution" aria-label="Parametres de l'execution">
      <h2>Execution</h2>
      <dl>
        <dt>trace</dt>
        <dd title={session.nom}>{session.nom}</dd>
        {debut && (
          <>
            <dt>programme</dt>
            <dd>{debut.programme || '-'}</dd>
            <dt>strategie</dt>
            <dd>{debut.strategie}</dd>
            <dt>graine</dt>
            <dd className="chiffres">{debut.graine}</dd>
            <dt>workers</dt>
            <dd>
              {debut.workers} (actifs : {workers.length})
            </dd>
          </>
        )}
        <dt>reactions</dt>
        <dd>{reactions}</dd>
        {envois > 0 && (
          <>
            <dt>envois</dt>
            <dd title="molecules envoyees par injection asynchrone (s:ref)">{envois} molecule(s) asynchrone(s)</dd>
          </>
        )}
        {retirees > 0 && (
          <>
            <dt>usage unique</dt>
            <dd title="activations de regles replace-one appliquees puis retirees">{retirees} regle(s) retiree(s)</dd>
          </>
        )}
        <dt>duree</dt>
        <dd>{(duree * 1000).toFixed(2)} ms</dd>
        <dt>fin</dt>
        <dd>{fin ? `${fin.motif}${fin.inerte ? ' (inerte)' : ''}` : 'absente'}</dd>
      </dl>
      {session.avertissements.length > 0 && (
        <details className="avertissements">
          <summary>{session.avertissements.length} avertissement(s)</summary>
          <ul>
            {session.avertissements.slice(0, 50).map((a, i) => (
              <li key={i}>{a}</li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}

export function TableauComptes({ session, position }: { session: Session; position: number }) {
  const { positionsParRegle, positionsParWorker, regles, workers } = session.statistiques;
  return (
    <details className="tableau">
      <summary>Tableau des reactions</summary>
      <table>
        <caption>Reactions jusqu'a l'etape {position} et sur toute la trace</caption>
        <thead>
          <tr>
            <th scope="col">categorie</th>
            <th scope="col">jusqu'a l'etape</th>
            <th scope="col">total</th>
          </tr>
        </thead>
        <tbody>
          {regles.map((r) => {
            const p = positionsParRegle.get(r) ?? [];
            return (
              <tr key={`r-${r}`}>
                <th scope="row">regle {r}</th>
                <td>{compterJusqua(p, position)}</td>
                <td>{p.length}</td>
              </tr>
            );
          })}
          {workers.map((w) => {
            const p = positionsParWorker.get(w) ?? [];
            return (
              <tr key={`w-${w}`}>
                <th scope="row">worker {w}</th>
                <td>{compterJusqua(p, position)}</td>
                <td>{p.length}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </details>
  );
}
