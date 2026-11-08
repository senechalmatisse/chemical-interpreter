#!/bin/bash

# FONCTION : Tests de concurrence repetes : chaque programme a sous-solutions,
#            dissolution, injection (synchrone ou asynchrone) ou regle a usage
#            unique est execute N fois en parallele ; le
#            resultat est compare au resultat attendu et la trace est
#            re-executee et verifiee (chem replay --programme)
#
# PARAMETRES : Ce programme prend en parametre
# (facultatif) 1: le nombre d'executions par programme (defaut : 100)
# (facultatif) 2: le nombre de workers (defaut : 4)
#
# La variable d'environnement CHEM_BINAIRE permet d'utiliser un executable
# deja construit (sinon : construction de engine/target/release/chem).
#

V_C_Repertoire_Depot="$(cd "$(dirname "$0")/.." && pwd)"
V_N_Nb_Executions="${1:-100}"
V_N_Nb_Workers="${2:-4}"
V_C_Binaire="${CHEM_BINAIRE:-${V_C_Repertoire_Depot}/engine/target/release/chem}"
V_C_Repertoire_Travail="$(mktemp -d)"
V_N_Nb_Echecs=0
V_N_Nb_Tests=0
V_N_Delai_Max=60

trap 'rm -rf "${V_C_Repertoire_Travail}"' EXIT

if [ ! -x "${V_C_Binaire}" ]
then
  ( cd "${V_C_Repertoire_Depot}/engine" && cargo build --release ) || exit 1
fi

# Execute un programme une fois et verifie resultat et trace
f_executer() {
  local P_E_C_Programme="$1"
  local P_E_N_Graine="$2"
  local V_C_Attendu
  local V_C_Obtenu
  local V_C_Trace="${V_C_Repertoire_Travail}/trace.jsonl"
  local V_N_Code_Retour
  V_C_Attendu="$(sed -n 's/^# attendu : //p' "${P_E_C_Programme}")"
  V_C_Obtenu="$(timeout "${V_N_Delai_Max}" "${V_C_Binaire}" run "${P_E_C_Programme}" \
    --workers "${V_N_Nb_Workers}" --seed "${P_E_N_Graine}" --trace-jsonl "${V_C_Trace}")"
  V_N_Code_Retour=$?
  (( V_N_Nb_Tests += 1 ))
  if [ "${V_N_Code_Retour}" -eq "124" ]
  then
    echo "ECHEC ${P_E_C_Programme} graine ${P_E_N_Graine} : delai depasse (interblocage ?)"
    (( V_N_Nb_Echecs += 1 ))
    return
  fi
  if [ "${V_N_Code_Retour}" -ne "0" ] || [ "${V_C_Obtenu}" != "${V_C_Attendu}" ]
  then
    echo "ECHEC ${P_E_C_Programme} graine ${P_E_N_Graine} : '${V_C_Obtenu}' au lieu de '${V_C_Attendu}'"
    (( V_N_Nb_Echecs += 1 ))
    return
  fi
  if ! "${V_C_Binaire}" replay "${V_C_Trace}" --programme "${P_E_C_Programme}" > /dev/null 2>&1
  then
    echo "ECHEC ${P_E_C_Programme} graine ${P_E_N_Graine} : trace refusee par le rejeu verifie"
    (( V_N_Nb_Echecs += 1 ))
  fi
}

V_N_Debut="$(date +%s)"
for V_C_Programme in b03_crible b06_dissolution b07_injection b08_max_min b15_imbrication \
  b17_usage_unique b18_sans_reactif b19_injection_asynchrone b20_injection_bornee
do
  echo "== ${V_C_Programme} : ${V_N_Nb_Executions} executions, ${V_N_Nb_Workers} workers"
  for (( V_N_Graine = 1; V_N_Graine <= V_N_Nb_Executions; V_N_Graine++ ))
  do
    f_executer "${V_C_Repertoire_Depot}/exemples/${V_C_Programme}.chem" "${V_N_Graine}"
  done
done
V_N_Duree=$(( $(date +%s) - V_N_Debut ))

echo "${V_N_Nb_Tests} execution(s), ${V_N_Nb_Echecs} echec(s), ${V_N_Duree} s"
if [ "${V_N_Nb_Echecs}" -eq "0" ]
then
  exit 0
fi
exit 1
