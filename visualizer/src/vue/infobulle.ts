// Infobulle partagee par les graphiques D3. Le contenu est insere avec
// textContent : les noms de regles viennent de la trace (donnee non sure).

export interface LigneInfobulle {
  readonly valeur: string;
  readonly libelle: string;
  readonly couleur?: string;
}

export class Infobulle {
  private readonly element: HTMLDivElement;

  constructor(conteneur: HTMLElement) {
    this.element = document.createElement('div');
    this.element.className = 'infobulle';
    this.element.setAttribute('role', 'status');
    this.element.hidden = true;
    conteneur.appendChild(this.element);
  }

  afficher(x: number, y: number, titre: string, lignes: readonly LigneInfobulle[]): void {
    this.element.replaceChildren();
    const entete = document.createElement('div');
    entete.className = 'infobulle-titre';
    entete.textContent = titre;
    this.element.appendChild(entete);
    for (const l of lignes) {
      const ligne = document.createElement('div');
      ligne.className = 'infobulle-ligne';
      if (l.couleur) {
        const cle = document.createElement('span');
        cle.className = 'infobulle-cle';
        cle.style.background = l.couleur;
        ligne.appendChild(cle);
      }
      const valeur = document.createElement('strong');
      valeur.textContent = l.valeur;
      const libelle = document.createElement('span');
      libelle.textContent = ` ${l.libelle}`;
      ligne.append(valeur, libelle);
      this.element.appendChild(ligne);
    }
    this.element.hidden = false;
    const parent = this.element.parentElement;
    const largeurParent = parent?.clientWidth ?? 0;
    const largeur = this.element.offsetWidth;
    const gauche = x + 14 + largeur > largeurParent ? x - largeur - 14 : x + 14;
    this.element.style.left = `${Math.max(0, gauche)}px`;
    this.element.style.top = `${Math.max(0, y - 10)}px`;
  }

  masquer(): void {
    this.element.hidden = true;
  }

  detruire(): void {
    this.element.remove();
  }
}
