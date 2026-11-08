//! Compilation : AST verifie -> regles compilees et solution initiale.
//!
//! Les variables sont resolues en emplacements, les produits en operations
//! sur les reactifs, et chaque regle n'est compilee qu'une fois puis
//! partagee par `Arc` (Poids-mouche). La solution initiale est assemblee
//! avec le [`ConstructeurSolution`] ; ses expressions constantes et ses
//! intervalles sont evalues ici.

use crate::ast::*;
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::evaluateur::{ExprCompilee, Valeur};
use crate::moteur::{Filtre, MotifCompile, ProduitCompile, RegleCompilee};
use crate::solution::{ConstructeurSolution, GenerateurIds, Solution};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Programme pret a etre execute.
#[derive(Debug)]
pub struct ProgrammeCompile {
    /// Regles par nom.
    pub regles: BTreeMap<String, Arc<RegleCompilee>>,
    /// Solution initiale.
    pub solution: Solution,
    /// Generateur d'identifiants (a reutiliser pendant l'execution).
    pub ids: GenerateurIds,
}

struct Portee {
    emplacements: HashMap<String, usize>,
    /// Sous-solutions liees (`s:sol`) : nom -> indice du motif.
    reactifs_solution: HashMap<String, usize>,
    /// References (`s:ref`) : nom -> rang de la reference.
    references: HashMap<String, usize>,
}

fn interne(message: &str) -> Diagnostic {
    Diagnostic::erreur(
        "E0399",
        format!("erreur interne de compilation : {message}"),
    )
    .aide("le programme aurait du etre rejete par l'analyse semantique ; merci de signaler ce cas")
}

fn compiler_motif(
    motif: &Motif,
    rang: usize,
    portee: &mut Portee,
) -> Result<MotifCompile, Diagnostic> {
    match motif {
        Motif::Variable {
            nom,
            type_annote:
                Some(TypeAnnote {
                    genre: GenreType::Sol,
                    ..
                }),
        } => {
            portee.reactifs_solution.insert(nom.nom.clone(), rang);
            Ok(MotifCompile::Solution)
        }
        Motif::Variable { nom, type_annote } => {
            let filtre = match type_annote.map(|t| t.genre) {
                None => Filtre::Valeur,
                Some(GenreType::Int) => Filtre::Entier,
                Some(GenreType::Bool) => Filtre::Booleen,
                Some(GenreType::Str) => Filtre::Chaine,
                Some(GenreType::Sol) => return Err(interne("motif 'sol' imbrique")),
                Some(GenreType::Ref) => return Err(interne("motif 'ref' imbrique")),
            };
            let emplacement = portee.emplacements.len();
            portee.emplacements.insert(nom.nom.clone(), emplacement);
            Ok(MotifCompile::Valeur {
                emplacement,
                filtre,
            })
        }
        Motif::NUplet { elements, .. } => {
            let mut compiles = Vec::with_capacity(elements.len());
            for m in elements {
                if let Motif::Variable {
                    type_annote:
                        Some(TypeAnnote {
                            genre: genre @ (GenreType::Sol | GenreType::Ref),
                            ..
                        }),
                    ..
                } = m
                {
                    return Err(interne(&format!("motif '{}' dans un n-uplet", genre.nom())));
                }
                compiles.push(compiler_motif(m, rang, portee)?);
            }
            Ok(MotifCompile::NUplet(compiles))
        }
    }
}

/// Compile une expression ; les variables sont resolues dans `emplacements`.
pub fn compiler_expression(
    expr: &Expr,
    emplacements: &HashMap<String, usize>,
) -> Result<ExprCompilee, Diagnostic> {
    Ok(match &expr.genre {
        GenreExpr::Entier(n) => ExprCompilee::Constante(Valeur::Entier(*n)),
        GenreExpr::Booleen(b) => ExprCompilee::Constante(Valeur::Booleen(*b)),
        GenreExpr::Chaine(s) => ExprCompilee::Constante(Valeur::chaine(s)),
        GenreExpr::Variable(nom) => ExprCompilee::Variable(
            *emplacements
                .get(nom)
                .ok_or_else(|| interne(&format!("variable '{nom}' non resolue")))?,
        ),
        GenreExpr::NUplet(elements) => ExprCompilee::NUplet(
            elements
                .iter()
                .map(|e| compiler_expression(e, emplacements))
                .collect::<Result<_, _>>()?,
        ),
        GenreExpr::Unaire { op, operande } => ExprCompilee::Unaire {
            op: *op,
            operande: Box::new(compiler_expression(operande, emplacements)?),
            span: expr.span,
        },
        GenreExpr::Binaire { op, gauche, droite } => ExprCompilee::Binaire {
            op: *op,
            gauche: Box::new(compiler_expression(gauche, emplacements)?),
            droite: Box::new(compiler_expression(droite, emplacements)?),
            span: expr.span,
        },
    })
}

/// Compile une definition de regle.
pub fn compiler_regle(definition: &Definition) -> Result<RegleCompilee, Diagnostic> {
    let regle = &definition.regle;
    let mut portee = Portee {
        emplacements: HashMap::new(),
        reactifs_solution: HashMap::new(),
        references: HashMap::new(),
    };
    // Les references ne sont pas des reactifs consommes : elles ne prennent
    // pas de motif, l'indice d'un motif ne compte que les elements filtres.
    let mut motifs = Vec::with_capacity(regle.reactifs.len());
    for m in &regle.reactifs {
        match m {
            Motif::Variable { nom, .. } if m.est_reference() => {
                let rang = portee.references.len();
                portee.references.insert(nom.nom.clone(), rang);
            }
            _ => {
                let rang = motifs.len();
                motifs.push(compiler_motif(m, rang, &mut portee)?);
            }
        }
    }
    let condition = regle
        .condition
        .as_ref()
        .map(|c| compiler_expression(c, &portee.emplacements))
        .transpose()?;
    let reactif = |nom: &Ident| {
        portee
            .reactifs_solution
            .get(&nom.nom)
            .copied()
            .ok_or_else(|| interne(&format!("'{}' n'est pas une sous-solution liee", nom.nom)))
    };
    let mut produits = Vec::with_capacity(regle.produits.len());
    for p in &regle.produits {
        produits.push(match p {
            Produit::Expression(Expr {
                genre: GenreExpr::Variable(nom),
                span,
            }) if portee.reactifs_solution.contains_key(nom) => ProduitCompile::Conservation {
                reactif: reactif(&Ident {
                    nom: nom.clone(),
                    span: *span,
                })?,
            },
            Produit::Expression(e) => {
                ProduitCompile::Valeur(compiler_expression(e, &portee.emplacements)?)
            }
            Produit::Dissolution { cible, .. } => ProduitCompile::Dissolution {
                reactif: reactif(cible)?,
            },
            Produit::Injection { valeur, cible, .. } => {
                let valeur = compiler_expression(valeur, &portee.emplacements)?;
                match portee.references.get(&cible.nom) {
                    Some(&reference) => ProduitCompile::Envoi { valeur, reference },
                    None => ProduitCompile::Injection {
                        valeur,
                        reactif: reactif(cible)?,
                    },
                }
            }
        });
    }
    Ok(RegleCompilee {
        nom: Arc::from(definition.nom.nom.as_str()),
        motifs,
        condition,
        produits,
        emplacements: portee.emplacements.len(),
        span: definition.span,
        unique: regle.unique,
        references: portee.references.len(),
    })
}

struct Assembleur<'a> {
    regles: &'a BTreeMap<String, Arc<RegleCompilee>>,
    ids: &'a GenerateurIds,
    max_molecules: u64,
    molecules: u64,
    diagnostics: Diagnostics,
}

impl Assembleur<'_> {
    fn evaluer_constante(&mut self, e: &Expr) -> Option<Valeur> {
        let compilee = match compiler_expression(e, &HashMap::new()) {
            Ok(c) => c,
            Err(d) => {
                self.diagnostics.ajouter(d);
                return None;
            }
        };
        match compilee.evaluer(&[]) {
            Ok(v) => Some(v),
            Err(erreur) => {
                self.diagnostics.ajouter(
                    Diagnostic::erreur("E0313", format!("evaluation impossible : {erreur}"))
                        .etiquette(e.span, "expression constante invalide"),
                );
                None
            }
        }
    }

    fn reserver(&mut self, nombre: u64, span: crate::source::Span) -> bool {
        if self.molecules.saturating_add(nombre) > self.max_molecules {
            self.diagnostics.ajouter(
                Diagnostic::erreur(
                    "E0315",
                    format!(
                        "solution initiale trop grande (limite : {} molecules)",
                        self.max_molecules
                    ),
                )
                .etiquette(span, format!("{nombre} molecule(s) de plus"))
                .aide("augmentez --max-molecules ou reduisez l'intervalle"),
            );
            return false;
        }
        self.molecules += nombre;
        true
    }

    fn assembler(&mut self, litterale: &SolutionLitterale) -> Option<Solution> {
        let mut constructeur = ConstructeurSolution::nouvelle(self.ids);
        for r in &litterale.regles {
            match self.regles.get(&r.nom) {
                Some(regle) => constructeur.ajouter_regle(Arc::clone(regle)),
                None => self
                    .diagnostics
                    .ajouter(interne(&format!("regle '{}' absente", r.nom))),
            }
        }
        for element in &litterale.elements {
            match element {
                Element::Expression(e) => {
                    if let Some(v) = self.evaluer_constante(e)
                        && self.reserver(1, e.span)
                    {
                        constructeur.ajouter_valeur(v);
                    }
                }
                Element::Intervalle { debut, fin, span } => {
                    let (Some(a), Some(b)) =
                        (self.evaluer_constante(debut), self.evaluer_constante(fin))
                    else {
                        continue;
                    };
                    let (Some(a), Some(b)) = (a.comme_entier(), b.comme_entier()) else {
                        self.diagnostics
                            .ajouter(interne("borne d'intervalle non entiere"));
                        continue;
                    };
                    if a > b {
                        self.diagnostics.ajouter(
                            Diagnostic::erreur("E0314", format!("intervalle vide {a}..{b}"))
                                .etiquette(*span, "la borne inferieure depasse la borne superieure")
                                .aide(format!("ecrivez {b}..{a}")),
                        );
                        continue;
                    }
                    let nombre = (b as i128 - a as i128 + 1).min(u64::MAX as i128) as u64;
                    if self.reserver(nombre, *span) {
                        for n in a..=b {
                            constructeur.ajouter_valeur(Valeur::Entier(n));
                        }
                    }
                }
                Element::Solution(s) => {
                    if let Some(sous) = self.assembler(s) {
                        constructeur.ajouter_sous_solution(sous);
                    }
                }
            }
        }
        Some(constructeur.construire())
    }
}

/// Compile un programme verifie.
pub fn compiler(
    programme: &Programme,
    max_molecules: u64,
) -> Result<ProgrammeCompile, Diagnostics> {
    let mut diagnostics = Diagnostics::new();
    let mut regles = BTreeMap::new();
    for d in &programme.definitions {
        match compiler_regle(d) {
            Ok(r) => {
                regles.insert(d.nom.nom.clone(), Arc::new(r));
            }
            Err(e) => diagnostics.ajouter(e),
        }
    }
    if diagnostics.a_des_erreurs() {
        return Err(diagnostics);
    }
    let ids = GenerateurIds::new();
    let mut assembleur = Assembleur {
        regles: &regles,
        ids: &ids,
        max_molecules,
        molecules: 0,
        diagnostics,
    };
    let solution = assembleur.assembler(&programme.solution);
    let diagnostics = assembleur.diagnostics;
    match solution {
        Some(solution) if !diagnostics.a_des_erreurs() => Ok(ProgrammeCompile {
            regles,
            solution,
            ids,
        }),
        _ => Err(diagnostics),
    }
}
