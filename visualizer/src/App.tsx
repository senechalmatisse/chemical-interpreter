// Application : chargement d'une trace, puis espace de visualisation
// (scene des solutions, controles de lecture, graphiques, tableau).

import { useCallback, useEffect, useMemo, useRef, useState, type ChangeEvent, type DragEvent, type RefObject } from 'react';
import { BarreControles } from './composants/BarreControles.tsx';
import { Accueil, ChargeurTrace, chargerUrl, lireFichier } from './composants/Chargement.tsx';
import { CourbeMolecules } from './composants/CourbeMolecules.tsx';
import { FriseWorkers } from './composants/FriseWorkers.tsx';
import { HistogrammeBarres, type Barre } from './composants/HistogrammeBarres.tsx';
import { PanneauEtape, PanneauExecution, TableauComptes } from './composants/Panneaux.tsx';
import { VueSolutions } from './composants/VueSolutions.tsx';
import { Lecteur } from './lecteur/lecteur.ts';
import { effetsDe } from './modele/commandes.ts';
import { chargerSession, type Session } from './modele/session.ts';
import { compterJusqua } from './modele/statistiques.ts';
import { exporterPng, exporterSvg } from './utilitaires/export.ts';
import { useLecteur, useSombreSysteme } from './vue/crochets.ts';
import { couleurCategorie, resoudreTheme, type PreferenceTheme, type Theme } from './vue/theme.ts';

interface Chargee {
  readonly session: Session;
  readonly lecteur: Lecteur;
}

const CLE_THEME = 'chem-visualiseur-theme';

function lirePreference(): PreferenceTheme {
  try {
    const v = window.localStorage.getItem(CLE_THEME);
    return v === 'clair' || v === 'sombre' ? v : 'systeme';
  } catch {
    return 'systeme';
  }
}

function premiereEtapeUtile(session: Session): number {
  const index = session.evenements.findIndex((e) => e.ev !== 'debut' && e.ev !== 'init');
  return index < 0 ? session.historique.total : index;
}

export function App() {
  const [preference, setPreference] = useState<PreferenceTheme>(lirePreference);
  const theme = resoudreTheme(preference, useSombreSysteme());
  const [chargee, setChargee] = useState<Chargee | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [survol, setSurvol] = useState(false);
  const refSvg = useRef<SVGSVGElement | null>(null);

  useEffect(() => {
    document.documentElement.dataset['theme'] = theme.nom;
    try {
      window.localStorage.setItem(CLE_THEME, preference);
    } catch {
      // stockage indisponible : preference non memorisee
    }
  }, [theme.nom, preference]);

  const charger = useCallback((nom: string, texte: string) => {
    try {
      const session = chargerSession(nom, texte);
      const lecteur = new Lecteur(session.historique);
      lecteur.allerA(premiereEtapeUtile(session));
      setChargee((precedente) => {
        precedente?.lecteur.detruire();
        return { session, lecteur };
      });
      setErreur(null);
    } catch (e) {
      setErreur(`${nom} : ${(e as Error).message}`);
    }
  }, []);

  useEffect(() => {
    const url = new URLSearchParams(window.location.search).get('trace');
    if (url) {
      chargerUrl(url).then(
        (texte) => charger(url, texte),
        (e: unknown) => setErreur(String(e)),
      );
    }
  }, [charger]);

  useEffect(() => {
    const lecteur = chargee?.lecteur;
    if (!lecteur) return undefined;
    const clavier = (e: KeyboardEvent) => {
      const cible = e.target as HTMLElement | null;
      if (cible && ['INPUT', 'SELECT', 'TEXTAREA', 'BUTTON'].includes(cible.tagName)) return;
      const actions: Record<string, () => void> = {
        ' ': () => lecteur.basculer(),
        ArrowRight: () => lecteur.suivant(),
        ArrowLeft: () => lecteur.precedent(),
        Home: () => lecteur.debut(),
        End: () => lecteur.finir(),
      };
      const action = actions[e.key];
      if (action) {
        e.preventDefault();
        action();
      }
    };
    window.addEventListener('keydown', clavier);
    return () => window.removeEventListener('keydown', clavier);
  }, [chargee]);

  const deposer = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setSurvol(false);
    const fichier = e.dataTransfer.files[0];
    if (fichier) lireFichier(fichier, charger, setErreur);
  };

  const nomExport = chargee ? chargee.session.nom.replace(/\.[^.]+$/, '') : 'solutions';

  return (
    <div
      className={survol ? 'application survol' : 'application'}
      onDragOver={(e: DragEvent<HTMLDivElement>) => {
        e.preventDefault();
        setSurvol(true);
      }}
      onDragLeave={() => setSurvol(false)}
      onDrop={deposer}
    >
      <header className="entete">
        <h1>
          Visualiseur <span>chimique</span>
        </h1>
        <ChargeurTrace onTexte={charger} onErreur={setErreur} />
        <div className="actions">
          {chargee && (
            <>
              <button type="button" onClick={() => refSvg.current && exporterSvg(refSvg.current, nomExport, theme.surface)}>
                Exporter SVG
              </button>
              <button
                type="button"
                onClick={() => {
                  if (refSvg.current) exporterPng(refSvg.current, nomExport, theme.surface).catch((e: unknown) => setErreur(String(e)));
                }}
              >
                Exporter PNG
              </button>
            </>
          )}
          <label>
            <span className="visuellement-cache">Theme</span>
            <select value={preference} onChange={(e: ChangeEvent<HTMLSelectElement>) => setPreference(e.target.value as PreferenceTheme)}>
              <option value="systeme">theme du systeme</option>
              <option value="clair">theme clair</option>
              <option value="sombre">theme sombre</option>
            </select>
          </label>
        </div>
      </header>
      {erreur && (
        <p className="erreur" role="alert">
          {erreur}
        </p>
      )}
      {chargee ? (
        <Espace key={chargee.session.nom + chargee.session.evenements.length} {...chargee} theme={theme} refSvg={refSvg} />
      ) : (
        <Accueil />
      )}
    </div>
  );
}

interface ProprietesEspace extends Chargee {
  readonly theme: Theme;
  readonly refSvg: RefObject<SVGSVGElement | null>;
}

function Espace({ session, lecteur, theme, refSvg }: ProprietesEspace) {
  const instantane = useLecteur(lecteur);
  const { historique, statistiques } = session;
  // l'etat est mutable : l'instantane est recalcule a chaque version du lecteur
  const vue = useMemo(() => historique.etat.instantane(), [historique, instantane.version]);
  const effets = useMemo(() => effetsDe(instantane.courante?.evenement ?? null), [instantane.courante]);
  // pas-a-pas : animation lisible ; lecture : bornee par la cadence
  const duree = instantane.etat === 'lecture' ? Math.min(900, 1500 / instantane.vitesse) : 850;
  const position = instantane.position;

  const couleurWorker = useCallback(
    (w: number) => couleurCategorie(theme, statistiques.workers.indexOf(w)),
    [theme, statistiques.workers],
  );
  const barresRegles: Barre[] = statistiques.regles.map((r, i) => {
    const positions = statistiques.positionsParRegle.get(r) ?? [];
    return { cle: r, libelle: r, valeur: compterJusqua(positions, position), total: positions.length, couleur: couleurCategorie(theme, i) };
  });
  const barresWorkers: Barre[] = statistiques.workers.map((w) => {
    const positions = statistiques.positionsParWorker.get(w) ?? [];
    return { cle: String(w), libelle: `worker ${w}`, valeur: compterJusqua(positions, position), total: positions.length, couleur: couleurWorker(w) };
  });
  const allerA = useCallback((etape: number) => lecteur.allerA(etape), [lecteur]);

  return (
    <main className="espace">
      <BarreControles lecteur={lecteur} instantane={instantane} />
      <div className="scene">
        <figure className="carte carte-vue">
          <VueSolutions vue={vue} effets={effets} sens={instantane.sens} duree={duree} theme={theme} refSvg={refSvg} />
          <figcaption className="legende">
            <span>
              <i className="pastille" style={{ background: theme.molecule }} /> molecule
            </span>
            <span>
              <i className="pastille" style={{ background: theme.reactif }} /> {'\u2212'} reactif consomme
            </span>
            <span>
              <i className="pastille" style={{ background: theme.produit }} /> + produit
            </span>
            <span>
              <i className="pastille transit" style={{ background: theme.molecule, borderColor: theme.bordTransit }} /> en transit (envoi
              asynchrone)
            </span>
            <span>
              <i className="anneau" style={{ borderColor: theme.bordActif }} /> solution active
            </span>
            <span>
              <i className="anneau" style={{ borderColor: theme.bordInerte }} /> solution inerte
            </span>
            <span className="astuce">molette : zoom, glisser : deplacer</span>
          </figcaption>
        </figure>
        <aside className="colonne">
          <PanneauEtape
            courante={instantane.courante}
            suivante={historique.suivante}
            molecules={historique.etat.nombreMolecules}
            transit={historique.etat.nombreEnTransit}
          />
          <PanneauExecution session={session} />
        </aside>
      </div>
      <div className="graphiques">
        <figure className="carte">
          <figcaption>Molecules par etape</figcaption>
          <CourbeMolecules points={statistiques.courbe} molecules={historique.molecules} position={position} theme={theme} onAllerA={allerA} />
        </figure>
        <figure className="carte">
          <figcaption>Reactions par regle</figcaption>
          {barresRegles.length > 0 ? <HistogrammeBarres barres={barresRegles} unite="reaction(s)" theme={theme} /> : <p className="vide">aucune reaction</p>}
        </figure>
        <figure className="carte">
          <figcaption>Reactions par worker</figcaption>
          {barresWorkers.length > 0 ? <HistogrammeBarres barres={barresWorkers} unite="reaction(s)" theme={theme} /> : <p className="vide">aucune reaction</p>}
        </figure>
        <figure className="carte large">
          <figcaption>Frise des reactions par worker (temps reel ; la relecture suit l'ordre du fichier)</figcaption>
          {statistiques.frise.length > 0 ? (
            <FriseWorkers
              cases={statistiques.frise}
              workers={statistiques.workers}
              instant={statistiques.instants[position] ?? 0}
              theme={theme}
              couleurWorker={couleurWorker}
            />
          ) : (
            <p className="vide">aucune reaction</p>
          )}
        </figure>
      </div>
      <TableauComptes session={session} position={position} />
    </main>
  );
}
