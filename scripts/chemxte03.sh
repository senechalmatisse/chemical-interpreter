#!/bin/bash

# FONCTION : Rapport de couverture des tests du moteur (cargo-llvm-cov) :
#            resume sur la sortie standard, rapport HTML dans couverture/
#
# PARAMETRES : Ce programme ne prend pas de parametre
#
# Prerequis : cargo install cargo-llvm-cov ; rustup component add llvm-tools-preview
#

V_C_Repertoire_Depot="$(cd "$(dirname "$0")/.." && pwd)"
V_C_Repertoire_Rapport="${V_C_Repertoire_Depot}/couverture"

if ! cargo llvm-cov --version > /dev/null 2>&1
then
  echo "cargo-llvm-cov absent : cargo install cargo-llvm-cov"
  exit 1
fi

cd "${V_C_Repertoire_Depot}/engine" || exit 1
cargo llvm-cov --summary-only || exit 1
cargo llvm-cov report --html --output-dir "${V_C_Repertoire_Rapport}" || exit 1
echo "Rapport HTML : ${V_C_Repertoire_Rapport}/html/index.html"
exit 0
