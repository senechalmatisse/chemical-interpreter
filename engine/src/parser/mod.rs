//! Analyse syntaxique par descente recursive : jetons -> AST.
//!
//! Grammaire (EBNF complete dans docs/langage.md) :
//!
//! ```text
//! programme  = { definition } , solution ;
//! definition = "let" , ident , "=" , regle , ";" ;
//! regle      = ( "replace" | "replace-one" ) , [ motif , { "," , motif } ] , "by" ,
//!              [ produit , { "," , produit } ] , [ "if" , expr ] ;
//! motif      = ident , [ ":" , type ] | "(" , motif , "," , motif , { "," , motif } , ")" ;
//! type       = "int" | "bool" | "str" | "sol" | "ref" ;
//! produit    = "dissolve" "(" ident ")" | "inject" "(" expr "," ident ")" | expr ;
//! solution   = "{" , [ element , { "," , element } ] , [ "|" , ident , { "," , ident } ] , "}" ;
//! element    = solution | expr , [ ".." , expr ] ;
//! ```
//!
//! L'analyse s'arrete a la premiere erreur, toujours localisee ; la
//! profondeur d'imbrication est bornee pour qu'aucune entree ne puisse
//! epuiser la pile.

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexer::{self, GenreJeton as G, Jeton};
use crate::source::{Source, Span};

/// Imbrication maximale (parentheses, solutions, operateurs prefixes).
pub const PROFONDEUR_MAX: usize = 128;
/// Profondeur maximale d'un arbre d'expression.
pub const PROFONDEUR_EXPRESSION_MAX: usize = 512;

/// Analyse un programme complet.
pub fn analyser_programme(source: &Source) -> Result<Programme, Diagnostic> {
    let mut parseur = Parseur::new(source.texte())?;
    parseur.programme()
}

/// Analyse une solution litterale seule (utile pour les resultats attendus).
pub fn analyser_solution(texte: &str) -> Result<SolutionLitterale, Diagnostic> {
    let mut parseur = Parseur::new(texte)?;
    let solution = parseur.solution()?;
    parseur.fin_attendue("apres la solution")?;
    Ok(solution)
}

/// Analyse une expression seule.
pub fn analyser_expression(texte: &str) -> Result<Expr, Diagnostic> {
    let mut parseur = Parseur::new(texte)?;
    let expr = parseur.expression()?;
    parseur.fin_attendue("apres l'expression")?;
    Ok(expr)
}

struct Parseur<'s> {
    jetons: Vec<Jeton>,
    pos: usize,
    texte: &'s str,
    profondeur: usize,
}

impl<'s> Parseur<'s> {
    fn new(texte: &'s str) -> Result<Self, Diagnostic> {
        Ok(Self {
            jetons: lexer::analyser(texte)?,
            pos: 0,
            texte,
            profondeur: 0,
        })
    }

    // ----- outils -----

    fn courant(&self) -> &Jeton {
        &self.jetons[self.pos]
    }

    fn genre(&self) -> &G {
        &self.courant().genre
    }

    fn avancer(&mut self) -> Jeton {
        let jeton = self.jetons[self.pos].clone();
        if self.pos + 1 < self.jetons.len() {
            self.pos += 1;
        }
        jeton
    }

    fn est(&self, genre: &G) -> bool {
        std::mem::discriminant(self.genre()) == std::mem::discriminant(genre)
    }

    fn accepter(&mut self, genre: &G) -> Option<Span> {
        if self.est(genre) {
            Some(self.avancer().span)
        } else {
            None
        }
    }

    fn exiger(&mut self, genre: &G, contexte: &str) -> Result<Span, Diagnostic> {
        match self.accepter(genre) {
            Some(span) => Ok(span),
            None => Err(self.erreur_attendu(&format!("'{}'", genre.symbole()), contexte)),
        }
    }

    /// Zone a signaler : juste apres le jeton precedent si le jeton courant
    /// est sur une autre ligne (cas typique du ';' oublie), sinon le jeton.
    fn zone_erreur(&self) -> Span {
        let courant = self.courant();
        if self.pos > 0 {
            let precedent = &self.jetons[self.pos - 1];
            let entre = &self.texte[precedent.span.fin..courant.span.debut];
            if entre.contains('\n') || courant.genre == G::Fin {
                return Span::new(precedent.span.fin, precedent.span.fin);
            }
        }
        courant.span
    }

    fn erreur_attendu(&self, attendu: &str, contexte: &str) -> Diagnostic {
        let trouve = self.courant().genre.description();
        let contexte = if contexte.is_empty() {
            String::new()
        } else {
            format!(" {contexte}")
        };
        let accord = if attendu.starts_with("une ") {
            "attendue"
        } else {
            "attendu"
        };
        Diagnostic::erreur(
            "E0201",
            format!("{attendu} {accord}{contexte}, trouve {trouve}"),
        )
        .etiquette(self.zone_erreur(), format!("{attendu} {accord} ici"))
    }

    fn fin_attendue(&self, contexte: &str) -> Result<(), Diagnostic> {
        if self.est(&G::Fin) {
            Ok(())
        } else {
            Err(Diagnostic::erreur(
                "E0203",
                format!(
                    "texte inattendu {contexte} : {}",
                    self.courant().genre.description()
                ),
            )
            .etiquette(self.courant().span, "rien ne doit suivre ici"))
        }
    }

    fn entrer(&mut self) -> Result<(), Diagnostic> {
        self.profondeur += 1;
        if self.profondeur > PROFONDEUR_MAX {
            return Err(Diagnostic::erreur(
                "E0207",
                format!("imbrication trop profonde (limite : {PROFONDEUR_MAX})"),
            )
            .etiquette(self.courant().span, "limite atteinte ici"));
        }
        Ok(())
    }

    fn sortir(&mut self) {
        self.profondeur -= 1;
    }

    fn identificateur(&mut self, contexte: &str) -> Result<Ident, Diagnostic> {
        if let G::Ident(nom) = self.genre() {
            let nom = nom.clone();
            let span = self.avancer().span;
            return Ok(Ident { nom, span });
        }
        let mut d = self.erreur_attendu("un identificateur", contexte);
        if self.courant().genre.est_mot_cle() {
            d = d.aide(format!(
                "'{}' est un mot reserve et ne peut pas servir de nom",
                self.courant().genre.symbole()
            ));
        }
        Err(d)
    }

    // ----- programme -----

    fn programme(&mut self) -> Result<Programme, Diagnostic> {
        let mut definitions = Vec::new();
        while self.est(&G::Let) {
            definitions.push(self.definition()?);
        }
        if !self.est(&G::AccoladeOuvrante) {
            let message = if self.est(&G::Fin) {
                "solution principale manquante : un programme se termine par une solution '{ ... }'"
                    .to_string()
            } else {
                format!(
                    "'let' ou '{{' attendu, trouve {}",
                    self.courant().genre.description()
                )
            };
            return Err(Diagnostic::erreur("E0202", message)
                .etiquette(self.zone_erreur(), "solution attendue ici")
                .aide("exemple : { 4, 17, 9 | max }"));
        }
        let solution = self.solution()?;
        self.fin_attendue("apres la solution principale")?;
        Ok(Programme {
            definitions,
            solution,
        })
    }

    fn definition(&mut self) -> Result<Definition, Diagnostic> {
        let debut = self.exiger(&G::Let, "")?;
        let nom = self.identificateur("apres 'let'")?;
        self.exiger(&G::Egal, "apres le nom de la regle")?;
        let regle = self.regle()?;
        let fin = self
            .exiger(&G::PointVirgule, "a la fin de la definition")
            .map_err(|d| d.aide("chaque definition 'let' se termine par ';'"))?;
        Ok(Definition {
            nom,
            regle,
            span: debut.fusion(fin),
        })
    }

    fn regle(&mut self) -> Result<Regle, Diagnostic> {
        let (debut, unique) = if let Some(span) = self.accepter(&G::ReplaceOne) {
            (span, true)
        } else if let Some(span) = self.accepter(&G::Replace) {
            (span, false)
        } else {
            return Err(self
                .erreur_attendu("'replace' ou 'replace-one'", "au debut d'une regle")
                .aide("exemple : let max = replace x, y by x if x >= y ;"));
        };
        // Les reactifs sont facultatifs : `replace by 1` est une regle sans reactif.
        let mut reactifs = Vec::new();
        if !self.est(&G::By) {
            reactifs.push(self.motif()?);
            while self.accepter(&G::Virgule).is_some() {
                reactifs.push(self.motif()?);
            }
        }
        let mut fin = self.exiger(&G::By, "apres les reactifs")?;
        let mut produits = Vec::new();
        if !self.est(&G::If) && !self.est(&G::PointVirgule) && !self.est(&G::Fin) {
            produits.push(self.produit()?);
            while self.accepter(&G::Virgule).is_some() {
                produits.push(self.produit()?);
            }
        }
        if let Some(p) = produits.last() {
            fin = p.span();
        }
        let condition = if self.accepter(&G::If).is_some() {
            let c = self.expression()?;
            fin = c.span;
            Some(c)
        } else {
            None
        };
        Ok(Regle {
            unique,
            reactifs,
            produits,
            condition,
            span: debut.fusion(fin),
        })
    }

    fn motif(&mut self) -> Result<Motif, Diagnostic> {
        if let Some(ouvrante) = self.accepter(&G::ParentheseOuvrante) {
            self.entrer()?;
            let mut elements = vec![self.motif()?];
            self.exiger(
                &G::Virgule,
                "dans un motif n-uplet (au moins deux composantes)",
            )?;
            elements.push(self.motif()?);
            while self.accepter(&G::Virgule).is_some() {
                elements.push(self.motif()?);
            }
            let fermante = self.exiger(&G::ParentheseFermante, "a la fin du motif n-uplet")?;
            self.sortir();
            return Ok(Motif::NUplet {
                elements,
                span: ouvrante.fusion(fermante),
            });
        }
        let nom = self.identificateur("comme reactif (variable ou motif n-uplet)")?;
        let type_annote = if self.accepter(&G::DeuxPoints).is_some() {
            Some(self.type_annote()?)
        } else {
            None
        };
        Ok(Motif::Variable { nom, type_annote })
    }

    fn type_annote(&mut self) -> Result<TypeAnnote, Diagnostic> {
        const DISPONIBLES: &str = "types disponibles : int, bool, str, sol, ref";
        let ident = self
            .identificateur("apres ':'")
            .map_err(|d| d.aide(DISPONIBLES))?;
        let Some(genre) = GenreType::depuis_nom(&ident.nom) else {
            return Err(
                Diagnostic::erreur("E0205", format!("type inconnu '{}'", ident.nom))
                    .etiquette(ident.span, "type non reconnu")
                    .aide(DISPONIBLES),
            );
        };
        Ok(TypeAnnote {
            genre,
            span: ident.span,
        })
    }

    fn produit(&mut self) -> Result<Produit, Diagnostic> {
        if let Some(debut) = self.accepter(&G::Dissolve) {
            self.exiger(&G::ParentheseOuvrante, "apres 'dissolve'")?;
            let cible = self.identificateur("dans 'dissolve(...)'")?;
            let fin = self.exiger(&G::ParentheseFermante, "apres la cible de 'dissolve'")?;
            return Ok(Produit::Dissolution {
                cible,
                span: debut.fusion(fin),
            });
        }
        if let Some(debut) = self.accepter(&G::Inject) {
            self.exiger(&G::ParentheseOuvrante, "apres 'inject'")?;
            let valeur = self.expression()?;
            self.exiger(
                &G::Virgule,
                "entre la valeur injectee et la sous-solution cible",
            )?;
            let cible = self.identificateur("comme sous-solution cible de 'inject'")?;
            let fin = self.exiger(&G::ParentheseFermante, "a la fin de 'inject(...)'")?;
            return Ok(Produit::Injection {
                valeur,
                cible,
                span: debut.fusion(fin),
            });
        }
        Ok(Produit::Expression(self.expression()?))
    }

    fn solution(&mut self) -> Result<SolutionLitterale, Diagnostic> {
        let debut = self.exiger(&G::AccoladeOuvrante, "au debut d'une solution")?;
        self.entrer()?;
        let mut elements = Vec::new();
        if !self.est(&G::Barre) && !self.est(&G::AccoladeFermante) {
            elements.push(self.element()?);
            while self.accepter(&G::Virgule).is_some() {
                elements.push(self.element()?);
            }
        }
        let mut regles = Vec::new();
        if self.accepter(&G::Barre).is_some() {
            regles.push(self.identificateur("apres '|' (nom de regle)")?);
            while self.accepter(&G::Virgule).is_some() {
                regles.push(self.identificateur("apres ',' (nom de regle)")?);
            }
        }
        let fin = self.exiger(&G::AccoladeFermante, "a la fin de la solution")?;
        self.sortir();
        Ok(SolutionLitterale {
            elements,
            regles,
            span: debut.fusion(fin),
        })
    }

    fn element(&mut self) -> Result<Element, Diagnostic> {
        if self.est(&G::AccoladeOuvrante) {
            return Ok(Element::Solution(self.solution()?));
        }
        let debut = self.expression()?;
        if self.accepter(&G::PointPoint).is_some() {
            let fin = self.expression()?;
            let span = debut.span.fusion(fin.span);
            return Ok(Element::Intervalle { debut, fin, span });
        }
        Ok(Element::Expression(debut))
    }

    // ----- expressions (priorites croissantes) -----

    fn expression(&mut self) -> Result<Expr, Diagnostic> {
        self.entrer()?;
        let expr = self.ou()?;
        self.sortir();
        if expr.profondeur() > PROFONDEUR_EXPRESSION_MAX {
            return Err(Diagnostic::erreur(
                "E0207",
                format!("expression trop profonde (limite : {PROFONDEUR_EXPRESSION_MAX} niveaux)"),
            )
            .etiquette(expr.span, "expression trop complexe"));
        }
        Ok(expr)
    }

    fn binaire(op: OpBinaire, gauche: Expr, droite: Expr) -> Expr {
        let span = gauche.span.fusion(droite.span);
        Expr {
            genre: GenreExpr::Binaire {
                op,
                gauche: Box::new(gauche),
                droite: Box::new(droite),
            },
            span,
        }
    }

    /// Borne la longueur d'une chaine d'operateurs binaires : chaque maillon
    /// approfondit l'arbre d'un niveau.
    fn maillon(&self, rang: &mut usize) -> Result<(), Diagnostic> {
        *rang += 1;
        if *rang > PROFONDEUR_EXPRESSION_MAX {
            return Err(Diagnostic::erreur(
                "E0207",
                format!("expression trop profonde (limite : {PROFONDEUR_EXPRESSION_MAX} niveaux)"),
            )
            .etiquette(self.courant().span, "expression trop longue"));
        }
        Ok(())
    }

    fn ou(&mut self) -> Result<Expr, Diagnostic> {
        let mut gauche = self.et()?;
        let mut rang = 0;
        while self.accepter(&G::Or).is_some() {
            self.maillon(&mut rang)?;
            let droite = self.et()?;
            gauche = Self::binaire(OpBinaire::Ou, gauche, droite);
        }
        Ok(gauche)
    }

    fn et(&mut self) -> Result<Expr, Diagnostic> {
        let mut gauche = self.non()?;
        let mut rang = 0;
        while self.accepter(&G::And).is_some() {
            self.maillon(&mut rang)?;
            let droite = self.non()?;
            gauche = Self::binaire(OpBinaire::Et, gauche, droite);
        }
        Ok(gauche)
    }

    fn non(&mut self) -> Result<Expr, Diagnostic> {
        if let Some(debut) = self.accepter(&G::Not) {
            self.entrer()?;
            let operande = self.non()?;
            self.sortir();
            return Ok(Expr {
                span: debut.fusion(operande.span),
                genre: GenreExpr::Unaire {
                    op: OpUnaire::Non,
                    operande: Box::new(operande),
                },
            });
        }
        self.comparaison()
    }

    fn operateur_comparaison(&self) -> Option<OpBinaire> {
        Some(match self.genre() {
            G::EgalEgal => OpBinaire::Egal,
            G::Different => OpBinaire::Different,
            G::Inferieur => OpBinaire::Inferieur,
            G::InferieurEgal => OpBinaire::InferieurEgal,
            G::Superieur => OpBinaire::Superieur,
            G::SuperieurEgal => OpBinaire::SuperieurEgal,
            _ => return None,
        })
    }

    fn comparaison(&mut self) -> Result<Expr, Diagnostic> {
        let gauche = self.additif()?;
        let Some(op) = self.operateur_comparaison() else {
            return Ok(gauche);
        };
        self.avancer();
        let droite = self.additif()?;
        let expr = Self::binaire(op, gauche, droite);
        if self.operateur_comparaison().is_some() {
            return Err(Diagnostic::erreur(
                "E0204",
                "les comparaisons ne s'enchainent pas sans parentheses",
            )
            .etiquette(self.courant().span, "seconde comparaison")
            .aide("ecrivez par exemple (a < b) and (b < c)"));
        }
        Ok(expr)
    }

    fn additif(&mut self) -> Result<Expr, Diagnostic> {
        let mut gauche = self.multiplicatif()?;
        let mut rang = 0;
        loop {
            let op = match self.genre() {
                G::Plus => OpBinaire::Plus,
                G::Moins => OpBinaire::Moins,
                _ => return Ok(gauche),
            };
            self.maillon(&mut rang)?;
            self.avancer();
            let droite = self.multiplicatif()?;
            gauche = Self::binaire(op, gauche, droite);
        }
    }

    fn multiplicatif(&mut self) -> Result<Expr, Diagnostic> {
        let mut gauche = self.unaire()?;
        let mut rang = 0;
        loop {
            let op = match self.genre() {
                G::Etoile => OpBinaire::Fois,
                G::Oblique => OpBinaire::Division,
                G::Mod => OpBinaire::Modulo,
                _ => return Ok(gauche),
            };
            self.maillon(&mut rang)?;
            self.avancer();
            let droite = self.unaire()?;
            gauche = Self::binaire(op, gauche, droite);
        }
    }

    fn unaire(&mut self) -> Result<Expr, Diagnostic> {
        if let Some(debut) = self.accepter(&G::Moins) {
            self.entrer()?;
            let operande = self.unaire()?;
            self.sortir();
            return Ok(Expr {
                span: debut.fusion(operande.span),
                genre: GenreExpr::Unaire {
                    op: OpUnaire::Moins,
                    operande: Box::new(operande),
                },
            });
        }
        self.primaire()
    }

    fn primaire(&mut self) -> Result<Expr, Diagnostic> {
        let span = self.courant().span;
        let genre = match self.genre().clone() {
            G::Entier(n) => GenreExpr::Entier(n),
            G::Chaine(s) => GenreExpr::Chaine(s),
            G::True => GenreExpr::Booleen(true),
            G::False => GenreExpr::Booleen(false),
            G::Ident(nom) => GenreExpr::Variable(nom),
            G::ParentheseOuvrante => return self.parentheses(),
            G::Dissolve | G::Inject => {
                return Err(Diagnostic::erreur(
                    "E0206",
                    format!(
                        "'{}' ne peut apparaitre que comme produit d'une regle",
                        self.genre().symbole()
                    ),
                )
                .etiquette(span, "utilisation interdite ici")
                .aide("exemple : replace s:sol by dissolve(s)"));
            }
            _ => return Err(self.erreur_attendu("une expression", "")),
        };
        self.avancer();
        Ok(Expr { genre, span })
    }

    fn parentheses(&mut self) -> Result<Expr, Diagnostic> {
        let debut = self.exiger(&G::ParentheseOuvrante, "")?;
        let premier = self.expression()?;
        if self.accepter(&G::Virgule).is_none() {
            let fin = self.exiger(&G::ParentheseFermante, "pour fermer la parenthese")?;
            return Ok(Expr {
                genre: premier.genre,
                span: debut.fusion(fin),
            });
        }
        let mut elements = vec![premier];
        loop {
            elements.push(self.expression()?);
            if self.accepter(&G::Virgule).is_none() {
                break;
            }
        }
        let fin = self.exiger(&G::ParentheseFermante, "a la fin du n-uplet")?;
        Ok(Expr {
            genre: GenreExpr::NUplet(elements),
            span: debut.fusion(fin),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Position;

    fn programme(texte: &str) -> Programme {
        analyser_programme(&Source::new("t", texte)).expect("programme valide")
    }

    fn erreur(texte: &str) -> (String, &'static str, Position) {
        let source = Source::new("t", texte);
        let d = analyser_programme(&source).expect_err("programme invalide");
        let pos = source.position(d.span().expect("localisee").debut);
        (d.message.clone(), d.code, pos)
    }

    #[test]
    fn programmes_de_reference() {
        let p = programme("let max = replace x, y by x if x >= y ;\n{ 4, 17, 9, 17, 2 | max }");
        assert_eq!(p.definitions.len(), 1);
        assert_eq!(p.definitions[0].nom.nom, "max");
        assert_eq!(p.definitions[0].regle.reactifs.len(), 2);
        assert!(p.definitions[0].regle.condition.is_some());
        assert_eq!(p.solution.elements.len(), 5);

        let p = programme(
            "let echanger = replace (i, a), (j, b) by (i, b), (j, a) if i < j and a > b ;\n{ (0, 5), (1, 3) | echanger }",
        );
        assert!(matches!(
            p.definitions[0].regle.reactifs[0],
            Motif::NUplet { .. }
        ));

        let p = programme(
            "let vider = replace s:sol by dissolve(s) ;\nlet envoyer = replace x:int, s:sol by inject(x, s);\n{ { 4, 17 | vider }, 2..5 | vider, envoyer }",
        );
        assert!(matches!(
            p.definitions[0].regle.produits[0],
            Produit::Dissolution { .. }
        ));
        assert!(matches!(p.solution.elements[1], Element::Intervalle { .. }));
    }

    #[test]
    fn regles_sans_produit() {
        let p = programme("let r = replace x, y by if x == y; let q = replace x by; { }");
        assert!(p.definitions[0].regle.produits.is_empty());
        assert!(p.definitions[1].regle.condition.is_none());
        assert!(p.solution.elements.is_empty());
    }

    #[test]
    fn regles_a_usage_unique_sans_reactif_et_references() {
        let p = programme(
            "let a = replace-one by 1, 2; let b = replace by if false; let c = replace-one x, s:ref by inject(x, s); { | a, b, c }",
        );
        let [a, b, c] = &p.definitions[..] else {
            panic!("trois definitions attendues")
        };
        assert!(a.regle.unique && a.regle.reactifs.is_empty());
        assert_eq!(a.regle.produits.len(), 2);
        assert!(!b.regle.unique && b.regle.reactifs.is_empty() && b.regle.produits.is_empty());
        assert!(b.regle.condition.is_some());
        assert!(c.regle.unique && c.regle.reactifs[1].est_reference());
        assert!(a.regle.ne_consomme_rien() && !c.regle.ne_consomme_rien());
        assert_eq!(a.regle.span, Span::new(8, 27));

        let (m, code, _) = erreur("let r = x by 1; { }");
        assert_eq!(code, "E0201");
        assert!(m.contains("'replace' ou 'replace-one'"), "{m}");
        let (_, code, _) = erreur("let r = replace by by; { }");
        assert_eq!(code, "E0201");
    }

    #[test]
    fn priorites() {
        let e = analyser_expression("1 + 2 * 3 == 7 and not false or x").unwrap();
        assert_eq!(e.to_string(), "1 + 2 * 3 == 7 and not false or x");
        match e.genre {
            GenreExpr::Binaire { op, .. } => assert_eq!(op, OpBinaire::Ou),
            _ => panic!("ou attendu a la racine"),
        }
    }

    #[test]
    fn erreurs_localisees() {
        let (m, c, p) = erreur("let max = replace x, y by x if x >= y\n{ 1 | max }");
        assert_eq!(c, "E0201");
        assert!(m.contains("';' attendu"), "{m}");
        assert_eq!(
            p,
            Position {
                ligne: 1,
                colonne: 38
            }
        );

        let (_, c, p) = erreur("let r = replace x y by x; { 1 }");
        assert_eq!(
            (c, p),
            (
                "E0201",
                Position {
                    ligne: 1,
                    colonne: 19
                }
            )
        );

        let (_, c, _) = erreur("let r = replace x by x;");
        assert_eq!(c, "E0202");

        let (_, c, p) = erreur("{ 1 } { 2 }");
        assert_eq!(
            (c, p),
            (
                "E0203",
                Position {
                    ligne: 1,
                    colonne: 7
                }
            )
        );

        let (_, c, _) = erreur("let r = replace x, y by x if x < y < 3; { 1 }");
        assert_eq!(c, "E0204");

        let (_, c, p) = erreur("let r = replace x:float by x; { 1 }");
        assert_eq!(
            (c, p),
            (
                "E0205",
                Position {
                    ligne: 1,
                    colonne: 19
                }
            )
        );

        let (_, c, _) = erreur("let r = replace s:sol by s + dissolve(s); { 1 }");
        assert_eq!(c, "E0206");

        let (_, c, _) = erreur("{ 1, 2, }");
        assert_eq!(c, "E0201");

        let (m, _, _) = erreur("let let = replace x by x; { }");
        assert!(m.contains("identificateur"));
    }

    #[test]
    fn imbrication_bornee() {
        let profond = format!("{{ {}1{} }}", "(".repeat(500), ")".repeat(500));
        assert_eq!(erreur(&profond).1, "E0207");
        let solutions = format!("{}{}", "{".repeat(300), "}".repeat(300));
        assert_eq!(erreur(&solutions).1, "E0207");
        let negations = format!("{{ {}1 }}", "-".repeat(1000));
        assert_eq!(erreur(&negations).1, "E0207");
        let longue = format!("{{ {} }}", vec!["1"; 2000].join(" + "));
        assert_eq!(erreur(&longue).1, "E0207");
    }

    #[test]
    fn solution_et_expression_seules() {
        assert_eq!(
            analyser_solution("{ 3, { 1 | r } | r }")
                .unwrap()
                .to_string(),
            "{ 3, { 1 | r } | r }"
        );
        assert!(analyser_solution("{ 1 } x").is_err());
        assert!(analyser_expression("1 +").is_err());
    }
}
