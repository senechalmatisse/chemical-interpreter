#!/bin/bash

# FONCTION : Lancement de la chaine de qualite complete : format (rustfmt),
#            analyse statique (clippy), tests du moteur, verification de
#            types et tests du visualiseur
#
# PARAMETRES : Ce programme ne prend pas de parametre
#

V_C_Repertoire_Depot="$(cd "$(dirname "$0")/.." && pwd)"
V_N_Nb_Echecs=0

# Execute une etape et comptabilise son echec eventuel
f_etape() {
  local P_E_C_Libelle="$1"
  shift
  local V_N_Code_Retour
  echo "== ${P_E_C_Libelle}"
  "$@"
  V_N_Code_Retour=$?
  if [ "${V_N_Code_Retour}" -ne "0" ]
  then
    echo "   ECHEC de l'etape (code ${V_N_Code_Retour})"
    (( V_N_Nb_Echecs += 1 ))
  fi
}

cd "${V_C_Repertoire_Depot}/engine" || exit 1
f_etape "Format du moteur (rustfmt)" cargo fmt --check
f_etape "Analyse statique (clippy)" cargo clippy --all-targets -- -D warnings
f_etape "Tests du moteur" cargo test

cd "${V_C_Repertoire_Depot}/visualizer" || exit 1
if [ -d "node_modules" ]
then
  f_etape "Verification de types du visualiseur" npm run typecheck
  f_etape "Tests du visualiseur" npm test
else
  echo "== Visualiseur ignore : dependances absentes (lancer 'npm install' dans visualizer/)"
fi

if [ "${V_N_Nb_Echecs}" -eq "0" ]
then
  echo "Chaine de qualite : succes"
  exit 0
fi
echo "Chaine de qualite : ${V_N_Nb_Echecs} etape(s) en echec"
exit 1
