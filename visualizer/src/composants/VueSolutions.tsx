// Vue des solutions : sous-solutions en conteneurs imbriques, molecules avec
// leur valeur. Une reaction est mise en evidence : les reactifs passent au
// rouge puis disparaissent, les produits apparaissent en vert a leur place.
// Les elements sont lies par identifiant stable : les deplacements
// (dissolution, reception du courrier, reorganisation) sont animes. Une
// molecule en transit (injection asynchrone) a un contour pointille et
// attend sur le bord de sa solution destinataire.

import * as d3 from 'd3';
import { useEffect, useRef, type RefObject } from 'react';
import type { Effets } from '../modele/commandes.ts';
import type { VueSolution } from '../modele/etat.ts';
import { formaterValeur, type ValeurMolecule } from '../trace/evenements.ts';
import { useTaille } from '../vue/crochets.ts';
import { disposer, Emplacements, type DisqueMolecule, type DisqueSolution } from '../vue/disposition.ts';
import type { Theme } from '../vue/theme.ts';

/** Au-dela, les transitions sont coupees pour rester fluide. */
const LIMITE_ANIMATION = 2500;
/** Bandeau reserve au-dessus de la solution principale pour son etiquette. */
const BANDEAU = 22;

function etiquetteMolecule(v: ValeurMolecule): string {
  const texte = formaterValeur(v);
  return texte.length > 9 ? `${texte.slice(0, 8)}\u2026` : texte;
}

export interface ProprietesVueSolutions {
  readonly vue: VueSolution | null;
  readonly effets: Effets;
  readonly sens: 'avant' | 'arriere' | 'saut';
  readonly duree: number;
  readonly theme: Theme;
  readonly refSvg: RefObject<SVGSVGElement | null>;
}

export function VueSolutions({ vue, effets, sens, duree, theme, refSvg }: ProprietesVueSolutions) {
  const [refBoite, taille] = useTaille<HTMLDivElement>();
  const zoomInstalle = useRef(false);
  const emplacements = useRef(new Emplacements());

  useEffect(() => {
    const svg = refSvg.current;
    if (!svg || taille.largeur === 0 || taille.hauteur === 0) return;
    const racineSvg = d3.select(svg);
    const zoomable = racineSvg
      .selectAll<SVGGElement, null>('g.zoom')
      .data([null])
      .join('g')
      .attr('class', 'zoom');
    if (!zoomInstalle.current) {
      racineSvg.call(
        d3
          .zoom<SVGSVGElement, unknown>()
          .scaleExtent([0.5, 24])
          .on('zoom', (e: d3.D3ZoomEvent<SVGSVGElement, unknown>) => {
            zoomable.attr('transform', e.transform.toString());
          }),
      );
      racineSvg.on('dblclick.zoom', null);
      zoomInstalle.current = true;
    }
    const couche = (nom: string) =>
      zoomable.selectAll<SVGGElement, null>(`g.${nom}`).data([null]).join('g').attr('class', nom);
    const coucheSolutions = couche('couche-solutions');
    const coucheMolecules = couche('couche-molecules');
    const coucheEtiquettes = couche('couche-etiquettes');

    if (!vue) {
      coucheSolutions.selectAll('*').remove();
      coucheMolecules.selectAll('*').remove();
      coucheEtiquettes.selectAll('*').remove();
      return;
    }

    const { solutions, molecules } = disposer(vue, taille.largeur, taille.hauteur, BANDEAU, emplacements.current);
    const anime = molecules.length <= LIMITE_ANIMATION && sens !== 'saut' && duree > 0;
    const d = anime ? duree : 0;
    const translation = (m: DisqueMolecule) => `translate(${m.x},${m.y})`;

    // --- conteneurs (sous-solutions) ---
    coucheSolutions
      .selectAll<SVGCircleElement, DisqueSolution>('circle.sol:not(.sortie)')
      .data(solutions, (s) => s.id)
      .join(
        (entree) => entree.append('circle').attr('class', 'sol').attr('r', 0).attr('cx', (s) => s.x).attr('cy', (s) => s.y),
        (maj) => maj,
        (sortie) => {
          sortie.attr('class', 'sol sortie').transition().duration(d).attr('r', 0).attr('opacity', 0).remove();
        },
      )
      .transition()
      .duration(d * 0.8)
      .attr('cx', (s) => s.x)
      .attr('cy', (s) => s.y)
      .attr('r', (s) => s.r)
      .attr('fill', (s) => theme.conteneurs[s.profondeur % theme.conteneurs.length]!)
      .attr('stroke', (s) => (s.vue.inerte ? theme.bordInerte : theme.bordActif))
      .attr('stroke-width', (s) => (effets.solutions.has(s.id) ? 3 : 1.5));

    // --- molecules ---
    const petit = molecules.length > 1500;
    const produite = (m: DisqueMolecule) => effets.produites.has(m.id);
    coucheMolecules
      .selectAll<SVGGElement, DisqueMolecule>('g.mol:not(.sortie)')
      .data(molecules, (m) => String(m.id))
      .join(
        (entree) => {
          const g = entree.append('g').attr('class', 'mol').attr('transform', translation);
          g.append('circle').attr('r', 0).attr('fill', theme.produit).attr('stroke-width', 1);
          g.append('text').attr('text-anchor', 'middle').attr('dy', '0.35em').attr('opacity', 0);
          g.append('title');
          return g;
        },
        (maj) => maj,
        (sortie) => {
          sortie.attr('class', 'mol sortie');
          sortie.select('circle').attr('fill', theme.reactif);
          sortie.select('text').attr('fill', theme.texteSurStatut);
          sortie
            .transition()
            .delay(d * 0.4)
            .duration(d * 0.45)
            .attr('opacity', 0)
            .remove()
            .select('circle')
            .attr('r', 0);
        },
      )
      .call((g) => {
        g.classed('transit', (m) => m.transit !== null);
        g.transition().duration(d * 0.8).attr('transform', translation);
        g.select('title').text(
          (m) => `molecule ${m.id} = ${formaterValeur(m.v)}${m.transit ? ` (en transit vers ${m.transit})` : ''}`,
        );
        g.select<SVGCircleElement>('circle')
          .attr('stroke', (m) => (m.transit ? theme.bordTransit : theme.surface))
          .attr('stroke-width', (m) => (m.transit ? 1.6 : 1))
          .attr('stroke-dasharray', (m) => (m.transit ? '3 2' : null))
          .attr('fill-opacity', (m) => (m.transit ? 0.8 : 1))
          .transition()
          .delay((m) => (produite(m) && sens === 'avant' ? d * 0.45 : 0))
          .duration(d * 0.5)
          .attr('r', (m) => m.r)
          .attr('fill', (m) => (produite(m) ? theme.produit : theme.molecule));
        g.select<SVGTextElement>('text')
          .text((m) => (petit ? '' : etiquetteMolecule(m.v)))
          .attr('font-size', (m) => Math.max(7, Math.min(15, m.r * 0.95, (m.r * 2.6) / Math.max(1, etiquetteMolecule(m.v).length))))
          .attr('fill', (m) => (produite(m) ? theme.texteSurStatut : theme.texteMolecule))
          .transition()
          .delay((m) => (produite(m) && sens === 'avant' ? d * 0.6 : 0))
          .duration(d * 0.3)
          .attr('opacity', (m) => (m.r >= 8 ? 1 : 0));
      });

    // --- etiquettes des solutions (au-dessus des molecules) ---
    coucheEtiquettes
      .selectAll<SVGTextElement, DisqueSolution>('text.etiquette')
      .data(
        solutions.filter((s) => s.profondeur === 0 || s.r >= 26),
        (s) => s.id,
      )
      .join((entree) => entree.append('text').attr('class', 'etiquette').attr('text-anchor', 'middle'))
      .text((s) => {
        const regles = s.vue.regles.length > 0 ? ` | ${s.vue.regles.join(', ')}` : '';
        return `${s.vue.id}${regles}${s.vue.inerte ? ' \u00b7 inerte' : ''}`;
      })
      .attr('fill', theme.encreSecondaire)
      .attr('stroke', theme.surface)
      .attr('stroke-width', 3)
      .attr('paint-order', 'stroke')
      .attr('font-size', (s) => (s.profondeur === 0 ? 13 : 11))
      .transition()
      .duration(d * 0.8)
      .attr('x', (s) => s.x)
      .attr('y', (s) => (s.profondeur === 0 ? s.y - s.r - 7 : s.y - s.r + 14));
  }, [vue, effets, sens, duree, theme, taille, refSvg]);

  return (
    <div className="vue-solutions" ref={refBoite}>
      <svg
        ref={refSvg}
        width={taille.largeur}
        height={taille.hauteur}
        role="img"
        aria-label="Solutions imbriquees et molecules de l'etape courante"
      />
    </div>
  );
}
