#!/bin/bash

# FONCTION : Mesures du pool Rayon (paquet integrations) face au pool de la
#            bibliotheque standard, de 1 worker au nombre de coeurs de la
#            machine ; tableau Markdown ecrit dans un fichier
#
# PARAMETRES : Ce programme prend en parametre
# (facultatif) 1: le fichier de resultat (defaut : docs/mesures-pools-AAAAMMJJ.md)
# (facultatif) 2: un filtre sur le nom des scenarios (ex. doublons)
#
# Les mesures demandent au moins 4 coeurs pour etre representatives, et le
# reseau au premier lancement (telechargement de Rayon).
#

V_C_Repertoire_Depot="$(cd "$(dirname "$0")/.." && pwd)"
V_C_Date="$(date +%Y%m%d)"
V_C_Fichier="${1:-${V_C_Repertoire_Depot}/docs/mesures-pools-${V_C_Date}.md}"
V_C_Filtre="${2:-}"
V_N_Nb_Coeurs="$(getconf _NPROCESSORS_ONLN 2> /dev/null || echo 1)"
V_N_Code_Retour=0

if [ "${V_N_Nb_Coeurs}" -lt "4" ]
then
  echo "Attention : ${V_N_Nb_Coeurs} coeur(s) disponible(s), 4 au moins pour des mesures representatives"
fi

cd "${V_C_Repertoire_Depot}/integrations" || exit 1
echo "Mesures en cours (quelques minutes)..."
if [ -n "${V_C_Filtre}" ]
then
  cargo bench --bench pools -- "${V_C_Filtre}" > "${V_C_Fichier}"
else
  cargo bench --bench pools > "${V_C_Fichier}"
fi
V_N_Code_Retour=$?
if [ "${V_N_Code_Retour}" -ne "0" ]
then
  echo "Echec des mesures (code ${V_N_Code_Retour})"
  exit "${V_N_Code_Retour}"
fi
{
  echo ""
  echo "Machine : $(uname -srm), ${V_N_Nb_Coeurs} coeur(s), $(rustc --version)"
  echo "Date : $(date +%Y-%m-%d)"
} >> "${V_C_Fichier}"
cat "${V_C_Fichier}"
echo "Resultats : ${V_C_Fichier}"
exit 0
