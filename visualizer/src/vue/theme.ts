// Jetons de couleur (palette de reference validee pour les daltonismes,
// modes clair et sombre). Les series suivent un ordre fixe et ne sont
// jamais recyclees : au-dela de 8, les categories sont regroupees.

export interface Theme {
  readonly nom: 'clair' | 'sombre';
  readonly surface: string;
  readonly plan: string;
  readonly encre: string;
  readonly encreSecondaire: string;
  readonly encreDiscrete: string;
  readonly grille: string;
  readonly axe: string;
  readonly series: readonly string[];
  readonly autres: string;
  readonly molecule: string;
  readonly texteMolecule: string;
  readonly reactif: string;
  readonly produit: string;
  readonly texteSurStatut: string;
  readonly conteneurs: readonly string[];
  readonly bordInerte: string;
  readonly bordActif: string;
  /** Contour (pointille) des molecules en transit. */
  readonly bordTransit: string;
}

export const THEME_CLAIR: Theme = {
  nom: 'clair',
  surface: '#fcfcfb',
  plan: '#f9f9f7',
  encre: '#0b0b0b',
  encreSecondaire: '#52514e',
  encreDiscrete: '#898781',
  grille: '#e1e0d9',
  axe: '#c3c2b7',
  series: ['#2a78d6', '#eb6834', '#1baf7a', '#eda100', '#e87ba4', '#008300', '#6250d6', '#e34948'],
  autres: '#898781',
  molecule: '#cde2fb',
  texteMolecule: '#0b0b0b',
  reactif: '#d03b3b',
  produit: '#0ca30c',
  texteSurStatut: '#ffffff',
  conteneurs: ['#f4f3ef', '#ebeae4', '#f4f3ef', '#ebeae4'],
  bordInerte: '#c3c2b7',
  bordActif: '#2a78d6',
  bordTransit: '#52514e',
};

export const THEME_SOMBRE: Theme = {
  nom: 'sombre',
  surface: '#1a1a19',
  plan: '#0d0d0d',
  encre: '#f0efec',
  encreSecondaire: '#c3c2b7',
  encreDiscrete: '#898781',
  grille: '#2c2c2a',
  axe: '#383835',
  series: ['#3987e5', '#d95926', '#199e70', '#c98500', '#d55181', '#008300', '#9085e9', '#e66767'],
  autres: '#898781',
  molecule: '#184f95',
  texteMolecule: '#f0efec',
  reactif: '#d03b3b',
  produit: '#0ca30c',
  texteSurStatut: '#ffffff',
  conteneurs: ['#232321', '#2c2c2a', '#232321', '#2c2c2a'],
  bordInerte: '#52514e',
  bordActif: '#3987e5',
  bordTransit: '#c3c2b7',
};

/** Couleur d'une categorie selon son rang d'apparition (jamais recyclee). */
export function couleurCategorie(theme: Theme, rang: number): string {
  return rang >= 0 && rang < theme.series.length ? theme.series[rang]! : theme.autres;
}

export type PreferenceTheme = 'systeme' | 'clair' | 'sombre';

export function resoudreTheme(preference: PreferenceTheme, sombreSysteme: boolean): Theme {
  if (preference === 'clair') return THEME_CLAIR;
  if (preference === 'sombre') return THEME_SOMBRE;
  return sombreSysteme ? THEME_SOMBRE : THEME_CLAIR;
}
