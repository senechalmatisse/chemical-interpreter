//! Analyse lexicale : transforme le texte source en jetons.
//!
//! Le [`Lexer`] est un iterateur (patron Iterator) qui produit des
//! `Result<Jeton, Diagnostic>` ; il s'arrete apres le jeton [`GenreJeton::Fin`]
//! ou apres la premiere erreur.

use crate::diagnostic::Diagnostic;
use crate::source::Span;

/// Nature d'un jeton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenreJeton {
    /// Litteral entier (toujours positif ; le signe est un operateur).
    Entier(i64),
    /// Litteral chaine, sequences d'echappement resolues.
    Chaine(String),
    /// Identificateur (nom de variable, de regle ou de type).
    Ident(String),
    /// `let`
    Let,
    /// `replace`
    Replace,
    /// `replace-one` (regle a usage unique)
    ReplaceOne,
    /// `by`
    By,
    /// `if`
    If,
    /// `and`
    And,
    /// `or`
    Or,
    /// `not`
    Not,
    /// `mod`
    Mod,
    /// `true`
    True,
    /// `false`
    False,
    /// `dissolve`
    Dissolve,
    /// `inject`
    Inject,
    /// `{`
    AccoladeOuvrante,
    /// `}`
    AccoladeFermante,
    /// `(`
    ParentheseOuvrante,
    /// `)`
    ParentheseFermante,
    /// `,`
    Virgule,
    /// `;`
    PointVirgule,
    /// `:`
    DeuxPoints,
    /// `=`
    Egal,
    /// `|`
    Barre,
    /// `..`
    PointPoint,
    /// `+`
    Plus,
    /// `-`
    Moins,
    /// `*`
    Etoile,
    /// `/`
    Oblique,
    /// `==`
    EgalEgal,
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
    /// Fin du texte.
    Fin,
}

impl GenreJeton {
    /// Mot cle correspondant a un mot, s'il y en a un.
    pub fn mot_cle(mot: &str) -> Option<GenreJeton> {
        use GenreJeton::*;
        Some(match mot {
            "let" => Let,
            "replace" => Replace,
            "replace-one" => ReplaceOne,
            "by" => By,
            "if" => If,
            "and" => And,
            "or" => Or,
            "not" => Not,
            "mod" => Mod,
            "true" => True,
            "false" => False,
            "dissolve" => Dissolve,
            "inject" => Inject,
            _ => return None,
        })
    }

    /// Vrai pour les mots reserves du langage.
    pub fn est_mot_cle(&self) -> bool {
        !self.symbole().is_empty() && Self::mot_cle(self.symbole()).is_some()
    }

    /// Ecriture du jeton dans le source (vide pour les jetons porteurs de valeur).
    pub fn symbole(&self) -> &'static str {
        use GenreJeton::*;
        match self {
            Entier(_) | Chaine(_) | Ident(_) | Fin => "",
            Let => "let",
            Replace => "replace",
            ReplaceOne => "replace-one",
            By => "by",
            If => "if",
            And => "and",
            Or => "or",
            Not => "not",
            Mod => "mod",
            True => "true",
            False => "false",
            Dissolve => "dissolve",
            Inject => "inject",
            AccoladeOuvrante => "{",
            AccoladeFermante => "}",
            ParentheseOuvrante => "(",
            ParentheseFermante => ")",
            Virgule => ",",
            PointVirgule => ";",
            DeuxPoints => ":",
            Egal => "=",
            Barre => "|",
            PointPoint => "..",
            Plus => "+",
            Moins => "-",
            Etoile => "*",
            Oblique => "/",
            EgalEgal => "==",
            Different => "!=",
            Inferieur => "<",
            InferieurEgal => "<=",
            Superieur => ">",
            SuperieurEgal => ">=",
        }
    }

    /// Description pour les messages d'erreur.
    pub fn description(&self) -> String {
        match self {
            GenreJeton::Entier(n) => format!("l'entier {n}"),
            GenreJeton::Chaine(_) => "une chaine".to_string(),
            GenreJeton::Ident(nom) => format!("l'identificateur '{nom}'"),
            GenreJeton::Fin => "la fin du fichier".to_string(),
            autre => format!("'{}'", autre.symbole()),
        }
    }
}

/// Jeton localise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jeton {
    /// Nature du jeton.
    pub genre: GenreJeton,
    /// Position dans le source.
    pub span: Span,
}

/// Analyseur lexical (iterateur de jetons).
pub struct Lexer<'a> {
    texte: &'a str,
    pos: usize,
    termine: bool,
}

impl<'a> Lexer<'a> {
    /// Cree un analyseur sur un texte.
    pub fn new(texte: &'a str) -> Self {
        Self {
            texte,
            pos: 0,
            termine: false,
        }
    }

    fn octet(&self, decalage: usize) -> Option<u8> {
        self.texte.as_bytes().get(self.pos + decalage).copied()
    }

    fn caractere(&self) -> Option<char> {
        self.texte[self.pos..].chars().next()
    }

    fn ignorer_blancs_et_commentaires(&mut self) {
        while let Some(o) = self.octet(0) {
            match o {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                b'#' => {
                    while let Some(c) = self.octet(0) {
                        if c == b'\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn jeton(&self, genre: GenreJeton, debut: usize) -> Result<Jeton, Diagnostic> {
        Ok(Jeton {
            genre,
            span: Span::new(debut, self.pos),
        })
    }

    fn jeton_suivant(&mut self) -> Result<Jeton, Diagnostic> {
        use GenreJeton::*;
        self.ignorer_blancs_et_commentaires();
        let debut = self.pos;
        let Some(o) = self.octet(0) else {
            return self.jeton(Fin, debut);
        };
        if o.is_ascii_digit() {
            return self.entier(debut);
        }
        if o.is_ascii_alphabetic() || o == b'_' {
            return self.mot(debut);
        }
        if o == b'"' {
            return self.chaine(debut);
        }
        let deux = |s: &mut Self, genre| {
            s.pos += 2;
            s.jeton(genre, debut)
        };
        let un = |s: &mut Self, genre| {
            s.pos += 1;
            s.jeton(genre, debut)
        };
        match (o, self.octet(1)) {
            (b'=', Some(b'=')) => deux(self, EgalEgal),
            (b'!', Some(b'=')) => deux(self, Different),
            (b'<', Some(b'=')) => deux(self, InferieurEgal),
            (b'>', Some(b'=')) => deux(self, SuperieurEgal),
            (b'.', Some(b'.')) => deux(self, PointPoint),
            (b'{', _) => un(self, AccoladeOuvrante),
            (b'}', _) => un(self, AccoladeFermante),
            (b'(', _) => un(self, ParentheseOuvrante),
            (b')', _) => un(self, ParentheseFermante),
            (b',', _) => un(self, Virgule),
            (b';', _) => un(self, PointVirgule),
            (b':', _) => un(self, DeuxPoints),
            (b'=', _) => un(self, Egal),
            (b'|', _) => un(self, Barre),
            (b'+', _) => un(self, Plus),
            (b'-', _) => un(self, Moins),
            (b'*', _) => un(self, Etoile),
            (b'/', _) => un(self, Oblique),
            (b'<', _) => un(self, Inferieur),
            (b'>', _) => un(self, Superieur),
            (b'.', _) => Err(Diagnostic::erreur("E0105", "'.' isole")
                .etiquette(Span::new(debut, debut + 1), "caractere inattendu")
                .aide("les intervalles s'ecrivent 'debut..fin', par exemple 2..100")),
            (b'!', _) => Err(Diagnostic::erreur("E0106", "'!' isole")
                .etiquette(Span::new(debut, debut + 1), "caractere inattendu")
                .aide("la difference s'ecrit '!=' et la negation 'not'")),
            _ => {
                let c = self.caractere().unwrap_or('?');
                let fin = debut + c.len_utf8();
                let mut d = Diagnostic::erreur("E0101", format!("caractere inattendu '{c}'"))
                    .etiquette(Span::new(debut, fin), "caractere non reconnu");
                if !c.is_ascii() {
                    d = d.aide(
                        "seuls les caracteres ASCII sont autorises hors commentaires et chaines",
                    );
                }
                Err(d)
            }
        }
    }

    fn entier(&mut self, debut: usize) -> Result<Jeton, Diagnostic> {
        while self.octet(0).is_some_and(|o| o.is_ascii_digit()) {
            self.pos += 1;
        }
        let texte = &self.texte[debut..self.pos];
        match texte.parse::<i64>() {
            Ok(n) => self.jeton(GenreJeton::Entier(n), debut),
            Err(_) => Err(Diagnostic::erreur("E0103", "entier trop grand")
                .etiquette(
                    Span::new(debut, self.pos),
                    "hors de l'intervalle des entiers 64 bits",
                )
                .aide(format!("la valeur maximale est {}", i64::MAX))),
        }
    }

    fn mot(&mut self, debut: usize) -> Result<Jeton, Diagnostic> {
        while self
            .octet(0)
            .is_some_and(|o| o.is_ascii_alphanumeric() || o == b'_')
        {
            self.pos += 1;
        }
        let mot = &self.texte[debut..self.pos];
        // `replace-one` forme un seul jeton : le tiret n'y est pas l'operateur
        // de soustraction (aucun motif ne commence par '-', pas d'ambiguite).
        if mot == "replace"
            && self.texte[self.pos..].starts_with("-one")
            && !self
                .octet(4)
                .is_some_and(|o| o.is_ascii_alphanumeric() || o == b'_')
        {
            self.pos += 4;
            return self.jeton(GenreJeton::ReplaceOne, debut);
        }
        let genre = GenreJeton::mot_cle(mot).unwrap_or_else(|| GenreJeton::Ident(mot.to_string()));
        self.jeton(genre, debut)
    }

    fn chaine(&mut self, debut: usize) -> Result<Jeton, Diagnostic> {
        self.pos += 1; // guillemet ouvrant
        let mut contenu = String::new();
        loop {
            let Some(c) = self.caractere() else {
                return Err(Diagnostic::erreur("E0102", "chaine non terminee")
                    .etiquette(Span::new(debut, debut + 1), "chaine ouverte ici")
                    .aide("ajoutez un '\"' fermant"));
            };
            match c {
                '"' => {
                    self.pos += 1;
                    return self.jeton(GenreJeton::Chaine(contenu), debut);
                }
                '\n' => {
                    return Err(Diagnostic::erreur("E0102", "chaine non terminee")
                        .etiquette(Span::new(debut, debut + 1), "chaine ouverte ici")
                        .aide("une chaine ne peut pas contenir de fin de ligne : utilisez \\n"));
                }
                '\\' => {
                    let pos_echappement = self.pos;
                    self.pos += 1;
                    let suivant = self.caractere();
                    let remplacement = match suivant {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        _ => {
                            let fin = self.pos + suivant.map_or(0, char::len_utf8);
                            return Err(Diagnostic::erreur(
                                "E0104",
                                "sequence d'echappement invalide",
                            )
                            .etiquette(Span::new(pos_echappement, fin), "echappement inconnu")
                            .aide("echappements reconnus : \\\" \\\\ \\n \\t \\r"));
                        }
                    };
                    self.pos += 1;
                    contenu.push(remplacement);
                }
                autre => {
                    self.pos += autre.len_utf8();
                    contenu.push(autre);
                }
            }
        }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Result<Jeton, Diagnostic>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.termine {
            return None;
        }
        let resultat = self.jeton_suivant();
        if matches!(
            &resultat,
            Ok(Jeton {
                genre: GenreJeton::Fin,
                ..
            }) | Err(_)
        ) {
            self.termine = true;
        }
        Some(resultat)
    }
}

/// Analyse un texte complet ; le dernier jeton est toujours [`GenreJeton::Fin`].
pub fn analyser(texte: &str) -> Result<Vec<Jeton>, Diagnostic> {
    Lexer::new(texte).collect()
}

#[cfg(test)]
mod tests {
    use super::GenreJeton::*;
    use super::*;

    fn genres(texte: &str) -> Vec<GenreJeton> {
        analyser(texte)
            .expect("lexemes valides")
            .into_iter()
            .map(|j| j.genre)
            .collect()
    }

    #[test]
    fn regle_complete() {
        assert_eq!(
            genres("let max = replace x, y by x if x >= y ;"),
            vec![
                Let,
                Ident("max".into()),
                Egal,
                Replace,
                Ident("x".into()),
                Virgule,
                Ident("y".into()),
                By,
                Ident("x".into()),
                If,
                Ident("x".into()),
                SuperieurEgal,
                Ident("y".into()),
                PointVirgule,
                Fin
            ]
        );
    }

    #[test]
    fn intervalles_commentaires_et_operateurs() {
        assert_eq!(
            genres("{ 2..50 | crible } # commentaire\n != == <= < > >= - * / mod"),
            vec![
                AccoladeOuvrante,
                Entier(2),
                PointPoint,
                Entier(50),
                Barre,
                Ident("crible".into()),
                AccoladeFermante,
                Different,
                EgalEgal,
                InferieurEgal,
                Inferieur,
                Superieur,
                SuperieurEgal,
                Moins,
                Etoile,
                Oblique,
                Mod,
                Fin
            ]
        );
    }

    #[test]
    fn chaines_et_echappements() {
        assert_eq!(
            genres("\"a\\\"b\\\\c\\n\" \"\u{e9}t\u{e9}\""),
            vec![
                Chaine("a\"b\\c\n".into()),
                Chaine("\u{e9}t\u{e9}".into()),
                Fin
            ]
        );
    }

    #[test]
    fn regle_a_usage_unique() {
        assert_eq!(
            genres("replace-one by 1"),
            vec![ReplaceOne, By, Entier(1), Fin]
        );
        // avec des blancs, le tiret redevient l'operateur
        assert_eq!(
            genres("replace - one"),
            vec![Replace, Moins, Ident("one".into()), Fin]
        );
        assert_eq!(
            genres("replace-oner"),
            vec![Replace, Moins, Ident("oner".into()), Fin]
        );
        let jetons = analyser("  replace-one x").unwrap();
        assert_eq!(jetons[0].span, Span::new(2, 13));
        assert!(ReplaceOne.est_mot_cle());
    }

    #[test]
    fn spans_exacts() {
        let jetons = analyser("  ab 12").unwrap();
        assert_eq!(jetons[0].span, Span::new(2, 4));
        assert_eq!(jetons[1].span, Span::new(5, 7));
        assert_eq!(jetons[2].span, Span::new(7, 7));
    }

    #[test]
    fn erreurs_lexicales() {
        let code = |t: &str| analyser(t).unwrap_err().code;
        assert_eq!(code("x @ y"), "E0101");
        assert_eq!(code("\u{e9}"), "E0101");
        assert_eq!(code("\"abc"), "E0102");
        assert_eq!(code("\"ab\nc\""), "E0102");
        assert_eq!(code("99999999999999999999"), "E0103");
        assert_eq!(code(r#""\q""#), "E0104");
        assert_eq!(code("1 . 2"), "E0105");
        assert_eq!(code("!x"), "E0106");
        assert_eq!(analyser("x @").unwrap_err().span(), Some(Span::new(2, 3)));
    }

    #[test]
    fn iterateur_s_arrete_apres_erreur() {
        let resultats: Vec<_> = Lexer::new("a @ b").collect();
        assert_eq!(resultats.len(), 2);
        assert!(resultats[1].is_err());
    }
}
