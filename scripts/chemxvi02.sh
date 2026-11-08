#!/bin/bash

# FONCTION : Verification des traces de demonstration du visualiseur : chaque
#            trace de l'index (visualizer/public/exemples/index.json) est
#            rejouee (chem replay) et la solution finale reconstruite est
#            comparee au resultat attendu ; toute trace du repertoire doit
#            figurer dans l'index
#
# PARAMETRES : Ce programme ne prend pas de parametre
#
# La variable d'environnement CHEM_BINAIRE permet d'utiliser un executable
# deja construit (sinon : construction de engine/target/release/chem).
#

V_C_Repertoire_Depot="$(cd "$(dirname "$0")/.." && pwd)"
V_C_Binaire="${CHEM_BINAIRE:-${V_C_Repertoire_Depot}/engine/target/release/chem}"
V_C_Repertoire_Exemples="${V_C_Repertoire_Depot}/visualizer/public/exemples"
V_C_Index="${V_C_Repertoire_Exemples}/index.json"
V_N_Nb_Traces=0
V_N_Nb_Echecs=0

if [ ! -x "${V_C_Binaire}" ]
then
  ( cd "${V_C_Repertoire_Depot}/engine" && cargo build --release ) || exit 1
fi
if [ ! -f "${V_C_Index}" ]
then
  echo "Index absent : ${V_C_Index}"
  exit 1
fi

# Rejoue une trace et compare la solution reconstruite au resultat attendu
f_verifier() {
  local P_E_C_Fichier="$1"
  local P_E_C_Attendu="$2"
  local V_C_Sortie
  local V_C_Obtenu
  local V_N_Code_Retour
  (( V_N_Nb_Traces += 1 ))
  V_C_Sortie="$("${V_C_Binaire}" replay "${V_C_Repertoire_Exemples}/${P_E_C_Fichier}" 2> /dev/null)"
  V_N_Code_Retour=$?
  V_C_Obtenu="$(printf '%s\n' "${V_C_Sortie}" | head -n 1)"
  if [ "${V_N_Code_Retour}" -ne "0" ]
  then
    echo "ECHEC ${P_E_C_Fichier} : trace refusee par le rejeu (code ${V_N_Code_Retour})"
    (( V_N_Nb_Echecs += 1 ))
  elif [ "${V_C_Obtenu}" != "${P_E_C_Attendu}" ]
  then
    echo "ECHEC ${P_E_C_Fichier} : '${V_C_Obtenu}' au lieu de '${P_E_C_Attendu}'"
    (( V_N_Nb_Echecs += 1 ))
  else
    echo "${P_E_C_Fichier} : ${V_C_Obtenu}"
  fi
}

# Une entree par ligne (format ecrit par chemxvi01.sh)
while IFS= read -r V_C_Ligne
do
  V_C_Fichier="$(printf '%s' "${V_C_Ligne}" | sed -nE 's/.*"fichier": "([^"]*)".*/\1/p')"
  if [ -n "${V_C_Fichier}" ]
  then
    V_C_Attendu="$(printf '%s' "${V_C_Ligne}" \
      | sed -nE 's/.*"attendu": "(([^"\\]|\\.)*)".*/\1/p' \
      | sed -e 's/\\"/"/g' -e 's/\\\\/\\/g')"
    f_verifier "${V_C_Fichier}" "${V_C_Attendu}"
  fi
done < "${V_C_Index}"

for V_C_Trace in "${V_C_Repertoire_Exemples}"/*.jsonl
do
  if ! grep -q "\"fichier\": \"$(basename "${V_C_Trace}")\"" "${V_C_Index}"
  then
    echo "ECHEC $(basename "${V_C_Trace}") : trace absente de l'index"
    (( V_N_Nb_Echecs += 1 ))
  fi
done

echo "${V_N_Nb_Traces} trace(s) rejouee(s), ${V_N_Nb_Echecs} echec(s)"
if [ "${V_N_Nb_Echecs}" -eq "0" ] && [ "${V_N_Nb_Traces}" -gt "0" ]
then
  exit 0
fi
exit 1
