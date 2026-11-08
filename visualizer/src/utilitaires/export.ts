// Export de la vue des solutions en image : SVG autonome (les
// couleurs sont des attributs, pas des classes CSS) ou PNG via un canevas.

function telecharger(url: string, nom: string): void {
  const lien = document.createElement('a');
  lien.href = url;
  lien.download = nom;
  document.body.appendChild(lien);
  lien.click();
  lien.remove();
}

function serialiser(svg: SVGSVGElement, fond: string): string {
  const copie = svg.cloneNode(true) as SVGSVGElement;
  copie.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  const largeur = svg.clientWidth || Number(svg.getAttribute('width')) || 800;
  const hauteur = svg.clientHeight || Number(svg.getAttribute('height')) || 600;
  copie.setAttribute('width', String(largeur));
  copie.setAttribute('height', String(hauteur));
  const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
  rect.setAttribute('width', '100%');
  rect.setAttribute('height', '100%');
  rect.setAttribute('fill', fond);
  copie.insertBefore(rect, copie.firstChild);
  copie.setAttribute('font-family', 'system-ui, -apple-system, "Segoe UI", sans-serif');
  return new XMLSerializer().serializeToString(copie);
}

export function exporterSvg(svg: SVGSVGElement, nom: string, fond: string): void {
  const blob = new Blob([serialiser(svg, fond)], { type: 'image/svg+xml' });
  const url = URL.createObjectURL(blob);
  telecharger(url, `${nom}.svg`);
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export function exporterPng(svg: SVGSVGElement, nom: string, fond: string, echelle = 2): Promise<void> {
  const texte = serialiser(svg, fond);
  const image = new Image();
  const source = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(texte)}`;
  return new Promise((resoudre, rejeter) => {
    image.onload = () => {
      const canevas = document.createElement('canvas');
      canevas.width = Math.round(image.width * echelle);
      canevas.height = Math.round(image.height * echelle);
      const contexte = canevas.getContext('2d');
      if (!contexte) {
        rejeter(new Error('canevas 2D indisponible'));
        return;
      }
      contexte.scale(echelle, echelle);
      contexte.drawImage(image, 0, 0);
      canevas.toBlob((blob) => {
        if (!blob) {
          rejeter(new Error('conversion PNG impossible'));
          return;
        }
        const url = URL.createObjectURL(blob);
        telecharger(url, `${nom}.png`);
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        resoudre();
      }, 'image/png');
    };
    image.onerror = () => rejeter(new Error('rendu SVG impossible'));
    image.src = source;
  });
}
