//! Arbre syntaxique abstrait (AST) des programmes chimiques.
//!
//! L'AST est produit par le [`crate::parser`], parcouru par les passes
//! semantiques (patron Visiteur, module [`visiteur`]) et reimprime en texte
//! par l'[`afficheur`] (test d'aller-retour).

pub mod afficheur;
pub mod visiteur;

use crate::source::Span;

/// Identificateur localise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    /// Nom.
    pub nom: String,
    /// Position.
    pub span: Span,
}

/// Programme : definitions de regles puis solution principale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Programme {
    /// Regles nommees (`let nom = replace ... ;`).
    pub definitions: Vec<Definition>,
    /// Solution initiale.
    pub solution: SolutionLitterale,
}

/// Definition d'une regle nommee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// Nom de la regle.
    pub nom: Ident,
    /// Corps de la regle.
    pub regle: Regle,
    /// Position de toute la definition.
    pub span: Span,
}

/// Regle de reaction : `replace reactifs by produits [if condition]`.
///
/// `replace-one` declare une regle a usage unique : chaque activation ne
/// s'applique qu'une fois, puis disparait de la solution (EF-D5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regle {
    /// Vrai pour `replace-one` (usage unique).
    pub unique: bool,
    /// Motifs des reactifs (eventuellement aucun : regle sans reactif).
    pub reactifs: Vec<Motif>,
    /// Produits (eventuellement aucun).
    pub produits: Vec<Produit>,
    /// Condition facultative.
    pub condition: Option<Expr>,
    /// Position de la regle.
    pub span: Span,
}

/// Motif filtrant un reactif.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Motif {
    /// Variable eventuellement typee : `x`, `x:int`, `s:sol`.
    Variable {
        /// Nom de la variable liee.
        nom: Ident,
        /// Type annote.
        type_annote: Option<TypeAnnote>,
    },
    /// N-uplet destructure : `(i, a)`.
    NUplet {
        /// Composantes (au moins deux).
        elements: Vec<Motif>,
        /// Position.
        span: Span,
    },
}

impl Motif {
    /// Position du motif complet.
    pub fn span(&self) -> Span {
        match self {
            Motif::Variable {
                nom,
                type_annote: Some(t),
            } => nom.span.fusion(t.span),
            Motif::Variable { nom, .. } => nom.span,
            Motif::NUplet { span, .. } => *span,
        }
    }
}

/// Type annote dans un motif.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeAnnote {
    /// Type.
    pub genre: GenreType,
    /// Position du nom de type.
    pub span: Span,
}

/// Types annotables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenreType {
    /// Entier relatif.
    Int,
    /// Booleen.
    Bool,
    /// Chaine.
    Str,
    /// Sous-solution (inerte), liee et consommee par la reaction.
    Sol,
    /// Reference a une sous-solution directe, inerte ou en cours
    /// d'execution, qui n'est pas consommee : cible d'une injection
    /// asynchrone (EF-D6).
    Ref,
}

impl GenreType {
    /// Nom dans le langage.
    pub fn nom(&self) -> &'static str {
        match self {
            GenreType::Int => "int",
            GenreType::Bool => "bool",
            GenreType::Str => "str",
            GenreType::Sol => "sol",
            GenreType::Ref => "ref",
        }
    }

    /// Types annotables, dans l'ordre de la documentation.
    pub const TOUS: [GenreType; 5] = [
        GenreType::Int,
        GenreType::Bool,
        GenreType::Str,
        GenreType::Sol,
        GenreType::Ref,
    ];

    /// Type designe par un nom (`int`, `bool`, `str`, `sol`, `ref`).
    pub fn depuis_nom(nom: &str) -> Option<GenreType> {
        Self::TOUS.into_iter().find(|t| t.nom() == nom)
    }
}

impl Motif {
    /// Vrai pour un motif `s:ref` (reference non consommee).
    pub fn est_reference(&self) -> bool {
        matches!(
            self,
            Motif::Variable {
                type_annote: Some(TypeAnnote {
                    genre: GenreType::Ref,
                    ..
                }),
                ..
            }
        )
    }
}

impl Regle {
    /// Vrai si la regle ne consomme aucun element : pas de reactif, ou
    /// seulement des references `s:ref`.
    pub fn ne_consomme_rien(&self) -> bool {
        self.reactifs.iter().all(Motif::est_reference)
    }
}

/// Produit d'une regle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Produit {
    /// Valeur produite dans la solution courante (ou `s` seul : conservation).
    Expression(Expr),
    /// `dissolve(s)` : libere le contenu de la sous-solution `s`.
    Dissolution {
        /// Sous-solution dissoute.
        cible: Ident,
        /// Position.
        span: Span,
    },
    /// `inject(e, s)` : produit la valeur `e` dans la sous-solution `s`.
    Injection {
        /// Valeur injectee.
        valeur: Expr,
        /// Sous-solution cible.
        cible: Ident,
        /// Position.
        span: Span,
    },
}

impl Produit {
    /// Position du produit.
    pub fn span(&self) -> Span {
        match self {
            Produit::Expression(e) => e.span,
            Produit::Dissolution { span, .. } | Produit::Injection { span, .. } => *span,
        }
    }
}

/// Solution ecrite dans le source : `{ elements | regles }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolutionLitterale {
    /// Elements (molecules, intervalles, sous-solutions).
    pub elements: Vec<Element>,
    /// Regles activees.
    pub regles: Vec<Ident>,
    /// Position.
    pub span: Span,
}

/// Element d'une solution litterale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    /// Molecule definie par une expression constante.
    Expression(Expr),
    /// Intervalle d'entiers `debut..fin` (bornes incluses).
    Intervalle {
        /// Borne inferieure.
        debut: Expr,
        /// Borne superieure.
        fin: Expr,
        /// Position.
        span: Span,
    },
    /// Sous-solution.
    Solution(SolutionLitterale),
}

/// Expression localisee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
    /// Nature de l'expression.
    pub genre: GenreExpr,
    /// Position.
    pub span: Span,
}

/// Nature d'une expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenreExpr {
    /// Entier.
    Entier(i64),
    /// Booleen.
    Booleen(bool),
    /// Chaine.
    Chaine(String),
    /// Reference a une variable liee par un motif.
    Variable(String),
    /// N-uplet `(a, b, ...)`.
    NUplet(Vec<Expr>),
    /// Operation unaire.
    Unaire {
        /// Operateur.
        op: OpUnaire,
        /// Operande.
        operande: Box<Expr>,
    },
    /// Operation binaire.
    Binaire {
        /// Operateur.
        op: OpBinaire,
        /// Operande gauche.
        gauche: Box<Expr>,
        /// Operande droite.
        droite: Box<Expr>,
    },
}

/// Operateurs unaires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpUnaire {
    /// `-`
    Moins,
    /// `not`
    Non,
}

/// Operateurs binaires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpBinaire {
    /// `+`
    Plus,
    /// `-`
    Moins,
    /// `*`
    Fois,
    /// `/`
    Division,
    /// `mod`
    Modulo,
    /// `==`
    Egal,
    /// `!=`
    Different,
    /// `<`
    Inferieur,
    /// `<=`
    InferieurEgal,
    /// `>`
    Superieur,
    /// `>=`
    SuperieurEgal,
    /// `and`
    Et,
    /// `or`
    Ou,
}

/// Niveaux de priorite (du plus faible au plus fort), documentes dans
/// docs/langage.md : `or` < `and` < `not` < comparaisons < `+ -` < `* / mod`
/// < `-` unaire < primaires.
pub mod priorite {
    /// `or`
    pub const OU: u8 = 1;
    /// `and`
    pub const ET: u8 = 2;
    /// `not`
    pub const NON: u8 = 3;
    /// `== != < <= > >=` (non associatifs)
    pub const COMPARAISON: u8 = 4;
    /// `+ -`
    pub const ADDITIF: u8 = 5;
    /// `* / mod`
    pub const MULTIPLICATIF: u8 = 6;
    /// `-` unaire
    pub const UNAIRE: u8 = 7;
    /// litteraux, variables, parentheses
    pub const PRIMAIRE: u8 = 8;
}

impl OpBinaire {
    /// Symbole dans le langage.
    pub fn symbole(&self) -> &'static str {
        use OpBinaire::*;
        match self {
            Plus => "+",
            Moins => "-",
            Fois => "*",
            Division => "/",
            Modulo => "mod",
            Egal => "==",
            Different => "!=",
            Inferieur => "<",
            InferieurEgal => "<=",
            Superieur => ">",
            SuperieurEgal => ">=",
            Et => "and",
            Ou => "or",
        }
    }

    /// Niveau de priorite (voir [`priorite`]).
    pub fn priorite(&self) -> u8 {
        use OpBinaire::*;
        match self {
            Ou => priorite::OU,
            Et => priorite::ET,
            Egal | Different | Inferieur | InferieurEgal | Superieur | SuperieurEgal => {
                priorite::COMPARAISON
            }
            Plus | Moins => priorite::ADDITIF,
            Fois | Division | Modulo => priorite::MULTIPLICATIF,
        }
    }

    /// Vrai pour les operateurs de comparaison (non associatifs).
    pub fn est_comparaison(&self) -> bool {
        self.priorite() == priorite::COMPARAISON
    }
}

impl OpUnaire {
    /// Symbole dans le langage.
    pub fn symbole(&self) -> &'static str {
        match self {
            OpUnaire::Moins => "-",
            OpUnaire::Non => "not",
        }
    }

    /// Niveau de priorite.
    pub fn priorite(&self) -> u8 {
        match self {
            OpUnaire::Moins => priorite::UNAIRE,
            OpUnaire::Non => priorite::NON,
        }
    }
}

impl Expr {
    /// Priorite de la racine de l'expression.
    pub fn priorite(&self) -> u8 {
        match &self.genre {
            GenreExpr::Binaire { op, .. } => op.priorite(),
            GenreExpr::Unaire { op, .. } => op.priorite(),
            _ => priorite::PRIMAIRE,
        }
    }

    /// Profondeur de l'arbre, calculee sans recursion (sure sur des
    /// expressions arbitrairement longues).
    pub fn profondeur(&self) -> usize {
        let mut pile = vec![(self, 1usize)];
        let mut maximum = 0;
        while let Some((e, p)) = pile.pop() {
            maximum = maximum.max(p);
            match &e.genre {
                GenreExpr::NUplet(es) => pile.extend(es.iter().map(|x| (x, p + 1))),
                GenreExpr::Unaire { operande, .. } => pile.push((operande, p + 1)),
                GenreExpr::Binaire { gauche, droite, .. } => {
                    pile.push((gauche, p + 1));
                    pile.push((droite, p + 1));
                }
                _ => {}
            }
        }
        maximum
    }
}

/// Copie d'un AST dont toutes les positions sont remises a zero : permet de
/// comparer deux AST independamment de la mise en page (aller-retour).
pub trait SansPositions {
    /// Retourne une copie sans positions.
    fn sans_positions(&self) -> Self;
}

const ZERO: Span = Span::new(0, 0);

impl SansPositions for Ident {
    fn sans_positions(&self) -> Self {
        Ident {
            nom: self.nom.clone(),
            span: ZERO,
        }
    }
}

impl SansPositions for Programme {
    fn sans_positions(&self) -> Self {
        Programme {
            definitions: self
                .definitions
                .iter()
                .map(|d| Definition {
                    nom: d.nom.sans_positions(),
                    regle: d.regle.sans_positions(),
                    span: ZERO,
                })
                .collect(),
            solution: self.solution.sans_positions(),
        }
    }
}

impl SansPositions for Regle {
    fn sans_positions(&self) -> Self {
        Regle {
            unique: self.unique,
            reactifs: self
                .reactifs
                .iter()
                .map(SansPositions::sans_positions)
                .collect(),
            produits: self
                .produits
                .iter()
                .map(SansPositions::sans_positions)
                .collect(),
            condition: self.condition.as_ref().map(SansPositions::sans_positions),
            span: ZERO,
        }
    }
}

impl SansPositions for Motif {
    fn sans_positions(&self) -> Self {
        match self {
            Motif::Variable { nom, type_annote } => Motif::Variable {
                nom: nom.sans_positions(),
                type_annote: type_annote.map(|t| TypeAnnote {
                    genre: t.genre,
                    span: ZERO,
                }),
            },
            Motif::NUplet { elements, .. } => Motif::NUplet {
                elements: elements.iter().map(SansPositions::sans_positions).collect(),
                span: ZERO,
            },
        }
    }
}

impl SansPositions for Produit {
    fn sans_positions(&self) -> Self {
        match self {
            Produit::Expression(e) => Produit::Expression(e.sans_positions()),
            Produit::Dissolution { cible, .. } => Produit::Dissolution {
                cible: cible.sans_positions(),
                span: ZERO,
            },
            Produit::Injection { valeur, cible, .. } => Produit::Injection {
                valeur: valeur.sans_positions(),
                cible: cible.sans_positions(),
                span: ZERO,
            },
        }
    }
}

impl SansPositions for SolutionLitterale {
    fn sans_positions(&self) -> Self {
        SolutionLitterale {
            elements: self
                .elements
                .iter()
                .map(|e| match e {
                    Element::Expression(x) => Element::Expression(x.sans_positions()),
                    Element::Intervalle { debut, fin, .. } => Element::Intervalle {
                        debut: debut.sans_positions(),
                        fin: fin.sans_positions(),
                        span: ZERO,
                    },
                    Element::Solution(s) => Element::Solution(s.sans_positions()),
                })
                .collect(),
            regles: self
                .regles
                .iter()
                .map(SansPositions::sans_positions)
                .collect(),
            span: ZERO,
        }
    }
}

impl SansPositions for Expr {
    fn sans_positions(&self) -> Self {
        let genre = match &self.genre {
            GenreExpr::NUplet(es) => {
                GenreExpr::NUplet(es.iter().map(SansPositions::sans_positions).collect())
            }
            GenreExpr::Unaire { op, operande } => GenreExpr::Unaire {
                op: *op,
                operande: Box::new(operande.sans_positions()),
            },
            GenreExpr::Binaire { op, gauche, droite } => GenreExpr::Binaire {
                op: *op,
                gauche: Box::new(gauche.sans_positions()),
                droite: Box::new(droite.sans_positions()),
            },
            autre => autre.clone(),
        };
        Expr { genre, span: ZERO }
    }
}
