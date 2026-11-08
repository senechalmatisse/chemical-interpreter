#!/bin/bash
# FONCTION : Generation des traces de demonstration du visualiseur
#            (visualizer/public/exemples/*.jsonl et index.json)
#
# PARAMETRES : Ce programme ne prend pas de parametre
#
# La variable d'environnement CHEM_BINAIRE permet d'utiliser un executable
# deja construit (sinon : construction de engine/target/release/chem).
#

V_C_Repertoire_Depot="$(cd "$(dirname "$0")/.." && pwd)"
V_C_Binaire="${CHEM_BINAIRE:-${V_C_Repertoire_Depot}/engine/target/release/chem}"
V_C_Sortie="${V_C_Repertoire_Depot}/visualizer/public/exemples"
V_C_Index="${V_C_Sortie}/index.json"
V_C_Crible="$(mktemp "${TMPDIR:-/tmp}/crible.XXXXXX")"
V_N_Premier=1

trap 'rm -f "${V_C_Crible}"' EXIT

if [ ! -x "${V_C_Binaire}" ]
then
  ( cd "${V_C_Repertoire_Depot}/engine" && cargo build --release ) || exit 1
fi
mkdir -p "${V_C_Sortie}"
printf '%s\n%s\n' "let crible = replace x, y by x if y mod x == 0 ;" "{ 2..1000 | crible }" > "${V_C_Crible}"

# Produit une trace et ajoute son entree a l'index
f_generer() {
  local P_E_C_Trace="$1"
  local P_E_C_Titre="$2"
  local P_E_C_Programme="$3"
  shift 3
  local V_C_Resultat
  V_C_Resultat="$("${V_C_Binaire}" run "${P_E_C_Programme}" "$@" --trace-jsonl "${V_C_Sortie}/${P_E_C_Trace}")" || return 1
  V_C_Resultat="$(printf '%s' "${V_C_Resultat}" | sed 's/\\/\\\\/g; s/"/\\"/g')"
  if [ "${V_N_Premier}" -eq "1" ]
  then
    V_N_Premier=0
  else
    printf ',\n' >> "${V_C_Index}"
  fi
  printf '  {"fichier": "%s", "titre": "%s", "attendu": "%s"}' \
    "${P_E_C_Trace}" "${P_E_C_Titre}" "${V_C_Resultat}" >> "${V_C_Index}"
  echo "${P_E_C_Trace} : ${V_C_Resultat}"
}

cd "${V_C_Repertoire_Depot}" || exit 1
printf '[\n' > "${V_C_Index}"
f_generer b09_crible_100.jsonl "B9 - crible jusqu'a 100 (1 worker)" exemples/b09_crible_100.chem --seed 7 || exit 1
f_generer b06_dissolution_w4.jsonl "B6 - maximums puis dissolution (4 workers)" exemples/b06_dissolution.chem --workers 4 --seed 2026 || exit 1
f_generer b07_injection.jsonl "B7 - injection et reactivation" exemples/b07_injection.chem --seed 3 || exit 1
f_generer b08_max_min_w2.jsonl "B8 - maximum et minimum en parallele (2 workers)" exemples/b08_max_min.chem --workers 2 --seed 5 || exit 1
f_generer b05_tri.jsonl "B5 - tri par echanges (n-uplets)" exemples/b05_tri.chem --strategie ordonnee || exit 1
f_generer b15_imbrication.jsonl "B15 - trois niveaux de dissolution (2 workers)" exemples/b15_imbrication.chem --workers 2 --seed 11 || exit 1
f_generer b17_usage_unique.jsonl "B17 - regles a usage unique (replace-one)" exemples/b17_usage_unique.chem --seed 4 || exit 1
f_generer b18_sans_reactif.jsonl "B18 - regle sans reactif dans une solution vide" exemples/b18_sans_reactif.chem --seed 2 || exit 1
f_generer b19_injection_asynchrone_w2.jsonl "B19 - injection asynchrone : crible au fil de l'eau (2 workers)" exemples/b19_injection_asynchrone.chem --workers 2 --seed 1 || exit 1
f_generer b20_injection_bornee_w2.jsonl "B20 - injections bornees : replace-one et s:ref (2 workers)" exemples/b20_injection_bornee.chem --workers 2 --seed 3 || exit 1
f_generer crible_1000_w4.jsonl "Crible jusqu'a 1000 (4 workers, 831 reactions)" "${V_C_Crible}" --workers 4 --seed 1 || exit 1
printf '\n]\n' >> "${V_C_Index}"
# remplacement portable (sans sed -i) du chemin temporaire du programme
sed "s#\"programme\":\"${V_C_Crible}\"#\"programme\":\"crible_1000.chem\"#" "${V_C_Sortie}/crible_1000_w4.jsonl" > "${V_C_Crible}.jsonl"
mv "${V_C_Crible}.jsonl" "${V_C_Sortie}/crible_1000_w4.jsonl"
echo "Index : ${V_C_Index}"
exit 0
