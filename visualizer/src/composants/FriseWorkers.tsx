// Frise temporelle des reactions par worker : une piste par worker,
// cases de duree egale dont l'opacite suit le nombre de reactions ; le trait
// vertical marque l'instant de l'etape courante.

import * as d3 from 'd3';
import { useEffect, useRef } from 'react';
import type { CaseFrise } from '../modele/statistiques.ts';
import { useTaille } from '../vue/crochets.ts';
import { Infobulle } from '../vue/infobulle.ts';
import type { Theme } from '../vue/theme.ts';

interface Proprietes {
  readonly cases: readonly CaseFrise[];
  readonly workers: readonly number[];
  readonly instant: number;
  readonly theme: Theme;
  readonly couleurWorker: (w: number) => string;
}

const PISTE = 22;
const ECART = 6;
const MARGES = { haut: 6, droite: 18, bas: 26, gauche: 52 };

const formaterMs = (secondes: number) => `${d3.format('~r')(+(secondes * 1000).toPrecision(3))} ms`;

export function FriseWorkers({ cases, workers, instant, theme, couleurWorker }: Proprietes) {
  const [refBoite, taille] = useTaille<HTMLDivElement>();
  const refSvg = useRef<SVGSVGElement | null>(null);
  const refInfobulle = useRef<Infobulle | null>(null);
  const hauteur = MARGES.haut + MARGES.bas + Math.max(1, workers.length) * (PISTE + ECART);

  useEffect(() => {
    const boite = refBoite.current;
    if (!boite) return undefined;
    refInfobulle.current = new Infobulle(boite);
    return () => refInfobulle.current?.detruire();
  }, [refBoite]);

  useEffect(() => {
    const svg = refSvg.current;
    if (!svg || taille.largeur === 0) return;
    const largeur = Math.max(10, taille.largeur - MARGES.gauche - MARGES.droite);
    const debut = d3.min(cases, (c) => c.debut) ?? 0;
    const fin = d3.max(cases, (c) => c.fin) ?? 1;
    const x = d3.scaleLinear().domain([debut, Math.max(fin, debut + 1e-6)]).range([0, largeur]);
    const rangPiste = new Map(workers.map((w, i) => [w, i]));
    const yPiste = (w: number) => MARGES.haut + (rangPiste.get(w) ?? 0) * (PISTE + ECART);
    const maximum = d3.max(cases, (c) => c.reactions) ?? 1;
    const g = d3
      .select(svg)
      .selectAll<SVGGElement, null>('g.frise')
      .data([null])
      .join('g')
      .attr('class', 'frise')
      .attr('transform', `translate(${MARGES.gauche},0)`);
    g.selectAll<SVGTextElement, number>('text.piste')
      .data(workers, (w) => String(w))
      .join('text')
      .attr('class', 'piste')
      .attr('x', -10)
      .attr('y', (w) => yPiste(w) + PISTE / 2)
      .attr('dy', '0.35em')
      .attr('text-anchor', 'end')
      .attr('fill', theme.encreSecondaire)
      .text((w) => `worker ${w}`);
    g.selectAll<SVGRectElement, number>('rect.fond')
      .data(workers, (w) => String(w))
      .join('rect')
      .attr('class', 'fond')
      .attr('x', 0)
      .attr('y', (w) => yPiste(w))
      .attr('width', largeur)
      .attr('height', PISTE)
      .attr('rx', 4)
      .attr('fill', theme.grille)
      .attr('fill-opacity', 0.5);
    g.selectAll<SVGRectElement, CaseFrise>('rect.case')
      .data(cases)
      .join('rect')
      .attr('class', 'case')
      .attr('x', (c) => x(c.debut))
      .attr('y', (c) => yPiste(c.worker) + 2)
      .attr('width', (c) => Math.max(1.5, x(c.fin) - x(c.debut) - 1))
      .attr('height', PISTE - 4)
      .attr('rx', 2)
      .attr('fill', (c) => couleurWorker(c.worker))
      .attr('fill-opacity', (c) => 0.25 + (0.75 * c.reactions) / maximum)
      .on('pointermove', (evenement: PointerEvent, c) => {
        const [bx, by] = d3.pointer(evenement, refBoite.current);
        refInfobulle.current?.afficher(bx, by, `worker ${c.worker}`, [
          { valeur: String(c.reactions), libelle: `reaction(s) entre ${formaterMs(c.debut)} et ${formaterMs(c.fin)}`, couleur: couleurWorker(c.worker) },
        ]);
      })
      .on('pointerleave', () => refInfobulle.current?.masquer());
    const basPistes = hauteur - MARGES.bas;
    g.selectAll<SVGGElement, null>('g.axe')
      .data([null])
      .join('g')
      .attr('class', 'axe')
      .attr('transform', `translate(0,${basPistes})`)
      .call(d3.axisBottom(x).ticks(Math.max(2, Math.floor(largeur / 110))).tickSizeOuter(0).tickFormat((t) => formaterMs(Number(t))))
      .call((sel) => sel.select('.domain').attr('stroke', theme.axe))
      .call((sel) => sel.selectAll('line').attr('stroke', theme.axe))
      .call((sel) => sel.selectAll('text').attr('fill', theme.encreDiscrete));
    const xInstant = Math.max(0, Math.min(largeur, x(instant)));
    g.selectAll<SVGLineElement, number>('line.instant')
      .data([xInstant])
      .join('line')
      .attr('class', 'instant')
      .attr('x1', (v) => v)
      .attr('x2', (v) => v)
      .attr('y1', MARGES.haut - 2)
      .attr('y2', basPistes)
      .attr('stroke', theme.encre)
      .attr('stroke-width', 1.5);
  }, [cases, workers, instant, theme, taille, couleurWorker, hauteur, refBoite]);

  return (
    <div className="graphique" ref={refBoite} style={{ height: hauteur }}>
      <svg ref={refSvg} width={taille.largeur} height={hauteur} role="img" aria-label="Frise des reactions par worker" />
    </div>
  );
}
