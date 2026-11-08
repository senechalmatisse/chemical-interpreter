// Barres horizontales (reactions par regle ou par worker) : piste claire pour
// le total de la trace, barre pleine pour le cumul a l'etape courante,
// valeur en bout de barre, infobulle par barre.

import * as d3 from 'd3';
import { useEffect, useRef } from 'react';
import { useTaille } from '../vue/crochets.ts';
import { Infobulle } from '../vue/infobulle.ts';
import type { Theme } from '../vue/theme.ts';

export interface Barre {
  readonly cle: string;
  readonly libelle: string;
  readonly valeur: number;
  readonly total: number;
  readonly couleur: string;
}

interface Proprietes {
  readonly barres: readonly Barre[];
  readonly unite: string;
  readonly theme: Theme;
}

const EPAISSEUR = 14;
const LIGNE = 30;
const MARGES = { haut: 6, droite: 70, bas: 6, gauche: 92 };

/** Rectangle a extremite de donnee arrondie (4 px), carre sur la ligne de base. */
function barreArrondie(x0: number, y0: number, largeur: number, hauteur: number, rayon = 4): string {
  if (largeur <= 0) return '';
  const r = Math.min(rayon, largeur, hauteur / 2);
  const x1 = x0 + largeur;
  return `M${x0},${y0}H${x1 - r}Q${x1},${y0} ${x1},${y0 + r}V${y0 + hauteur - r}Q${x1},${y0 + hauteur} ${x1 - r},${y0 + hauteur}H${x0}Z`;
}

export function HistogrammeBarres({ barres, unite, theme }: Proprietes) {
  const [refBoite, taille] = useTaille<HTMLDivElement>();
  const refSvg = useRef<SVGSVGElement | null>(null);
  const refInfobulle = useRef<Infobulle | null>(null);
  const hauteurTotale = MARGES.haut + MARGES.bas + Math.max(1, barres.length) * LIGNE;

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
    const maximum = d3.max(barres, (b) => b.total) ?? 1;
    const x = d3.scaleLinear().domain([0, Math.max(1, maximum)]).range([0, largeur]);
    const y = (i: number) => MARGES.haut + i * LIGNE + (LIGNE - EPAISSEUR) / 2;
    const g = d3
      .select(svg)
      .selectAll<SVGGElement, null>('g.barres')
      .data([null])
      .join('g')
      .attr('class', 'barres')
      .attr('transform', `translate(${MARGES.gauche},0)`);
    const lignes = g
      .selectAll<SVGGElement, Barre>('g.ligne')
      .data(barres, (b) => b.cle)
      .join((entree) => {
        const l = entree.append('g').attr('class', 'ligne');
        l.append('text').attr('class', 'libelle').attr('text-anchor', 'end').attr('dy', '0.35em');
        l.append('path').attr('class', 'piste');
        l.append('path').attr('class', 'plein');
        l.append('text').attr('class', 'valeur').attr('dy', '0.35em');
        l.append('rect').attr('class', 'cible').attr('fill', 'transparent');
        return l;
      });
    lignes
      .select<SVGTextElement>('text.libelle')
      .attr('x', -10)
      .attr('y', (_, i) => y(i) + EPAISSEUR / 2)
      .attr('fill', theme.encreSecondaire)
      .text((b) => b.libelle);
    lignes
      .select<SVGPathElement>('path.piste')
      .attr('d', (b, i) => barreArrondie(0, y(i), x(b.total), EPAISSEUR))
      .attr('fill', theme.grille);
    lignes
      .select<SVGPathElement>('path.plein')
      .attr('d', (b, i) => barreArrondie(0, y(i), x(b.valeur), EPAISSEUR))
      .attr('fill', (b) => b.couleur);
    lignes
      .select<SVGTextElement>('text.valeur')
      .attr('x', (b) => x(b.total) + 8)
      .attr('y', (_, i) => y(i) + EPAISSEUR / 2)
      .attr('fill', theme.encre)
      .text((b) => (b.valeur === b.total ? String(b.total) : `${b.valeur} / ${b.total}`));
    lignes
      .select<SVGRectElement>('rect.cible')
      .attr('x', -MARGES.gauche)
      .attr('y', (_, i) => MARGES.haut + i * LIGNE)
      .attr('width', MARGES.gauche + largeur + MARGES.droite)
      .attr('height', LIGNE)
      .on('pointermove', (evenement: PointerEvent, b) => {
        const [bx, by] = d3.pointer(evenement, refBoite.current);
        refInfobulle.current?.afficher(bx, by, b.libelle, [
          { valeur: String(b.valeur), libelle: `${unite} jusqu'a l'etape courante`, couleur: b.couleur },
          { valeur: String(b.total), libelle: `${unite} sur toute la trace` },
        ]);
      })
      .on('pointerleave', () => refInfobulle.current?.masquer());
  }, [barres, unite, theme, taille, refBoite]);

  return (
    <div className="graphique graphique-barres" ref={refBoite} style={{ height: hauteurTotale }}>
      <svg ref={refSvg} width={taille.largeur} height={hauteurTotale} role="img" aria-label={`Reactions par ${unite}`} />
    </div>
  );
}
