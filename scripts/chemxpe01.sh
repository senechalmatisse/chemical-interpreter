#!/bin/bash

# FONCTION : Mesures de performance du moteur (sequentiel contre parallele,
#            surcout de la trace) ; tableau Markdown ecrit dans un fichier
#
# PARAMETRES : Ce programme prend en parametre
# (facultatif) 1: le fichier de resultat (defaut : docs/mesures-AAAAMMJJ.md)
# (facultatif) 2: un filtre sur le nom des scenarios (ex. crible)
#

V_C_Repertoire_Depot="$(cd "$(dirname "$0")/.." && pwd)"
V_C_Date="$(date +%Y%m%d)"
V_C_Fichier="${1:-${V_C_Repertoire_Depot}/docs/mesures-${V_C_Date}.md}"
V_C_Filtre="${2:-}"
V_N_Code_Retour=0

cd "${V_C_Repertoire_Depot}/engine" || exit 1
echo "Mesures en cours (quelques minutes)..."
if [ -n "${V_C_Filtre}" ]
then
  cargo bench --bench moteur -- "${V_C_Filtre}" > "${V_C_Fichier}"
else
  cargo bench --bench moteur > "${V_C_Fichier}"
fi
V_N_Code_Retour=$?
if [ "${V_N_Code_Retour}" -ne "0" ]
then
  echo "Echec des mesures (code ${V_N_Code_Retour})"
  exit "${V_N_Code_Retour}"
fi
{
  echo ""
  echo "Machine : $(uname -srm), $(nproc) coeur(s), $(rustc --version)"
  echo "Date : $(date +%Y-%m-%d)"
} >> "${V_C_Fichier}"
cat "${V_C_Fichier}"
echo "Resultats : ${V_C_Fichier}"
exit 0
