// Crochets React partages.

import { useEffect, useRef, useState, useSyncExternalStore, type RefObject } from 'react';
import type { Instantane, Lecteur } from '../lecteur/lecteur.ts';

export interface Taille {
  readonly largeur: number;
  readonly hauteur: number;
}

/** Taille d'un element, suivie par ResizeObserver. */
export function useTaille<T extends Element>(): [RefObject<T | null>, Taille] {
  const ref = useRef<T | null>(null);
  const [taille, setTaille] = useState<Taille>({ largeur: 0, hauteur: 0 });
  useEffect(() => {
    const element = ref.current;
    if (!element) return undefined;
    const observateur = new ResizeObserver((entrees) => {
      const boite = entrees[0]?.contentRect;
      if (boite) setTaille({ largeur: Math.round(boite.width), hauteur: Math.round(boite.height) });
    });
    observateur.observe(element);
    return () => observateur.disconnect();
  }, []);
  return [ref, taille];
}

/** Instantane du lecteur (patron Observateur cote React). */
export function useLecteur(lecteur: Lecteur): Instantane {
  return useSyncExternalStore(lecteur.abonner, lecteur.instantane);
}

/** Vrai si le systeme demande un theme sombre. */
export function useSombreSysteme(): boolean {
  const requete = '(prefers-color-scheme: dark)';
  const [sombre, setSombre] = useState(() => window.matchMedia(requete).matches);
  useEffect(() => {
    const media = window.matchMedia(requete);
    const ecouter = (e: MediaQueryListEvent) => setSombre(e.matches);
    media.addEventListener('change', ecouter);
    return () => media.removeEventListener('change', ecouter);
  }, []);
  return sombre;
}
