// Nombre de molecules au fil des etapes. Ligne de 2 px, aplat a 10 %,
// reticule et infobulle au survol ; un clic deplace la lecture a l'etape.

import * as d3 from 'd3';
import { useEffect, useRef } from 'react';
import type { PointCourbe } from '../modele/statistiques.ts';
import { useTaille } from '../vue/crochets.ts';
import { Infobulle } from '../vue/infobulle.ts';
import type { Theme } from '../vue/theme.ts';

interface Proprietes {
  readonly points: readonly PointCourbe[];
  readonly molecules: readonly number[];
  readonly position: number;
  readonly theme: Theme;
  readonly onAllerA: (etape: number) => void;
}

const MARGES = { haut: 12, droite: 18, bas: 26, gauche: 52 };

export function CourbeMolecules({ points, molecules, position, theme, onAllerA }: Proprietes) {
  const [refBoite, taille] = useTaille<HTMLDivElement>();
  const refSvg = useRef<SVGSVGElement | null>(null);
  const refInfobulle = useRef<Infobulle | null>(null);

  useEffect(() => {
    const boite = refBoite.current;
    if (!boite) return undefined;
    refInfobulle.current = new Infobulle(boite);
    return () => refInfobulle.current?.detruire();
  }, [refBoite]);

  useEffect(() => {
    const svg = refSvg.current;
    if (!svg || taille.largeur === 0 || points.length === 0) return;
    const largeur = taille.largeur - MARGES.gauche - MARGES.droite;
    const hauteur = taille.hauteur - MARGES.haut - MARGES.bas;
    if (largeur <= 0 || hauteur <= 0) return;
    const total = points.at(-1)!.etape;
    const x = d3.scaleLinear().domain([0, Math.max(1, total)]).range([0, largeur]);
    const maximum = d3.max(points, (p) => p.molecules) ?? 1;
    const y = d3.scaleLinear().domain([0, Math.max(1, maximum)]).nice(4).range([hauteur, 0]);
    const racine = d3.select(svg);
    const g = racine
      .selectAll<SVGGElement, null>('g.trace')
      .data([null])
      .join('g')
      .attr('class', 'trace')
      .attr('transform', `translate(${MARGES.gauche},${MARGES.haut})`);

    g.selectAll<SVGGElement, null>('g.grille')
      .data([null])
      .join('g')
      .attr('class', 'grille')
      .call((sel) =>
        sel
          .selectAll<SVGLineElement, number>('line')
          .data(y.ticks(4))
          .join('line')
          .attr('x1', 0)
          .attr('x2', largeur)
          .attr('y1', (t) => y(t))
          .attr('y2', (t) => y(t))
          .attr('stroke', theme.grille)
          .attr('stroke-width', 1),
      );
    const axeY = d3.axisLeft(y).ticks(4).tickSize(0).tickPadding(8).tickFormat(d3.format('~s'));
    g.selectAll<SVGGElement, null>('g.axe-y')
      .data([null])
      .join('g')
      .attr('class', 'axe-y')
      .call(axeY)
      .call((sel) => sel.select('.domain').remove())
      .call((sel) => sel.selectAll('text').attr('fill', theme.encreDiscrete));
    const axeX = d3.axisBottom(x).ticks(Math.max(2, Math.floor(largeur / 90))).tickSizeOuter(0).tickFormat(d3.format('~s'));
    g.selectAll<SVGGElement, null>('g.axe-x')
      .data([null])
      .join('g')
      .attr('class', 'axe-x')
      .attr('transform', `translate(0,${hauteur})`)
      .call(axeX)
      .call((sel) => sel.select('.domain').attr('stroke', theme.axe))
      .call((sel) => sel.selectAll('line').attr('stroke', theme.axe))
      .call((sel) => sel.selectAll('text').attr('fill', theme.encreDiscrete));

    const aire = d3
      .area<PointCourbe>()
      .x((p) => x(p.etape))
      .y0(hauteur)
      .y1((p) => y(p.molecules))
      .curve(d3.curveStepAfter);
    const ligne = d3
      .line<PointCourbe>()
      .x((p) => x(p.etape))
      .y((p) => y(p.molecules))
      .curve(d3.curveStepAfter);
    const serie = theme.series[0]!;
    g.selectAll<SVGPathElement, readonly PointCourbe[]>('path.aire')
      .data([points])
      .join('path')
      .attr('class', 'aire')
      .attr('d', (p) => aire([...p]))
      .attr('fill', serie)
      .attr('fill-opacity', 0.1);
    g.selectAll<SVGPathElement, readonly PointCourbe[]>('path.ligne')
      .data([points])
      .join('path')
      .attr('class', 'ligne')
      .attr('d', (p) => ligne([...p]))
      .attr('fill', 'none')
      .attr('stroke', serie)
      .attr('stroke-width', 2)
      .attr('stroke-linejoin', 'round')
      .attr('stroke-linecap', 'round');

    // position courante : trait vertical et point avec anneau de surface
    const courant = molecules[Math.min(position, molecules.length - 1)] ?? 0;
    g.selectAll<SVGLineElement, number>('line.courant')
      .data([position])
      .join('line')
      .attr('class', 'courant')
      .attr('x1', (p) => x(p))
      .attr('x2', (p) => x(p))
      .attr('y1', 0)
      .attr('y2', hauteur)
      .attr('stroke', theme.encreDiscrete)
      .attr('stroke-width', 1);
    g.selectAll<SVGCircleElement, number>('circle.courant')
      .data([position])
      .join('circle')
      .attr('class', 'courant')
      .attr('cx', (p) => x(p))
      .attr('cy', y(courant))
      .attr('r', 4.5)
      .attr('fill', serie)
      .attr('stroke', theme.surface)
      .attr('stroke-width', 2);

    // reticule
    const reticule = g
      .selectAll<SVGLineElement, null>('line.reticule')
      .data([null])
      .join('line')
      .attr('class', 'reticule')
      .attr('y1', 0)
      .attr('y2', hauteur)
      .attr('stroke', theme.axe)
      .attr('stroke-width', 1)
      .attr('visibility', 'hidden');
    const etapeSous = (evenement: PointerEvent) => {
      const [px] = d3.pointer(evenement, g.node());
      return Math.max(0, Math.min(total, Math.round(x.invert(px))));
    };
    g.selectAll<SVGRectElement, null>('rect.capture')
      .data([null])
      .join('rect')
      .attr('class', 'capture')
      .attr('width', largeur)
      .attr('height', hauteur)
      .attr('fill', 'transparent')
      .style('cursor', 'pointer')
      .on('pointermove', (evenement: PointerEvent) => {
        const etape = etapeSous(evenement);
        reticule.attr('x1', x(etape)).attr('x2', x(etape)).attr('visibility', 'visible');
        const [bx, by] = d3.pointer(evenement, refBoite.current);
        refInfobulle.current?.afficher(bx, by, `etape ${etape}`, [
          { valeur: String(molecules[etape] ?? 0), libelle: 'molecules', couleur: serie },
        ]);
      })
      .on('pointerleave', () => {
        reticule.attr('visibility', 'hidden');
        refInfobulle.current?.masquer();
      })
      .on('click', (evenement: PointerEvent) => onAllerA(etapeSous(evenement)));
  }, [points, molecules, position, theme, taille, onAllerA, refBoite]);

  return (
    <div className="graphique" ref={refBoite}>
      <svg ref={refSvg} width={taille.largeur} height={taille.hauteur} role="img" aria-label="Nombre de molecules par etape" />
    </div>
  );
}
