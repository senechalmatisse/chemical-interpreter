//! JSON minimal (ecriture et lecture) pour la trace JSON Lines.
//!
//! Point d'integration : ce module joue le role de Serde JSON. Il ne couvre
//! que ce dont la trace a besoin ; les entiers sont conserves exactement
//! (pas de passage par `f64`) et la profondeur de lecture est bornee.

use std::fmt::Write;

/// Valeur JSON.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// `null`
    Nul,
    /// `true` / `false`
    Booleen(bool),
    /// Nombre entier.
    Entier(i64),
    /// Nombre a virgule.
    Reel(f64),
    /// Chaine.
    Chaine(String),
    /// Tableau.
    Tableau(Vec<Json>),
    /// Objet (ordre des champs conserve).
    Objet(Vec<(String, Json)>),
}

/// Profondeur maximale acceptee a la lecture.
const PROFONDEUR_MAX: usize = 64;

impl Json {
    /// Objet a partir de paires.
    pub fn objet(champs: Vec<(&str, Json)>) -> Json {
        Json::Objet(
            champs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    /// Champ d'un objet.
    pub fn champ(&self, nom: &str) -> Option<&Json> {
        match self {
            Json::Objet(champs) => champs.iter().find(|(k, _)| k == nom).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Entier (accepte un reel sans partie fractionnaire).
    pub fn comme_entier(&self) -> Option<i64> {
        match self {
            Json::Entier(n) => Some(*n),
            Json::Reel(r) if r.fract() == 0.0 && r.abs() < 9.0e15 => Some(*r as i64),
            _ => None,
        }
    }

    /// Entier positif.
    pub fn comme_u64(&self) -> Option<u64> {
        self.comme_entier().and_then(|n| u64::try_from(n).ok())
    }

    /// Nombre quelconque.
    pub fn comme_reel(&self) -> Option<f64> {
        match self {
            Json::Entier(n) => Some(*n as f64),
            Json::Reel(r) => Some(*r),
            _ => None,
        }
    }

    /// Chaine.
    pub fn comme_chaine(&self) -> Option<&str> {
        match self {
            Json::Chaine(s) => Some(s),
            _ => None,
        }
    }

    /// Booleen.
    pub fn comme_booleen(&self) -> Option<bool> {
        match self {
            Json::Booleen(b) => Some(*b),
            _ => None,
        }
    }

    /// Tableau.
    pub fn comme_tableau(&self) -> Option<&[Json]> {
        match self {
            Json::Tableau(t) => Some(t),
            _ => None,
        }
    }

    /// Serialisation compacte.
    pub fn ecrire(&self, sortie: &mut String) {
        match self {
            Json::Nul => sortie.push_str("null"),
            Json::Booleen(b) => sortie.push_str(if *b { "true" } else { "false" }),
            Json::Entier(n) => {
                let _ = write!(sortie, "{n}");
            }
            Json::Reel(r) if r.is_finite() => {
                let _ = write!(sortie, "{r}");
            }
            Json::Reel(_) => sortie.push_str("null"),
            Json::Chaine(s) => ecrire_chaine(sortie, s),
            Json::Tableau(elements) => {
                sortie.push('[');
                for (i, e) in elements.iter().enumerate() {
                    if i > 0 {
                        sortie.push(',');
                    }
                    e.ecrire(sortie);
                }
                sortie.push(']');
            }
            Json::Objet(champs) => {
                sortie.push('{');
                for (i, (k, v)) in champs.iter().enumerate() {
                    if i > 0 {
                        sortie.push(',');
                    }
                    ecrire_chaine(sortie, k);
                    sortie.push(':');
                    v.ecrire(sortie);
                }
                sortie.push('}');
            }
        }
    }

    /// Lecture d'un texte JSON complet.
    pub fn analyser(texte: &str) -> Result<Json, String> {
        let mut lecteur = Lecteur {
            octets: texte.as_bytes(),
            texte,
            pos: 0,
        };
        lecteur.blancs();
        let valeur = lecteur.valeur(0)?;
        lecteur.blancs();
        if lecteur.pos != texte.len() {
            return Err(format!("texte superflu a la position {}", lecteur.pos + 1));
        }
        Ok(valeur)
    }
}

impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        self.ecrire(&mut s);
        f.write_str(&s)
    }
}

fn ecrire_chaine(sortie: &mut String, s: &str) {
    sortie.push('"');
    for c in s.chars() {
        match c {
            '"' => sortie.push_str("\\\""),
            '\\' => sortie.push_str("\\\\"),
            '\n' => sortie.push_str("\\n"),
            '\r' => sortie.push_str("\\r"),
            '\t' => sortie.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(sortie, "\\u{:04x}", c as u32);
            }
            c => sortie.push(c),
        }
    }
    sortie.push('"');
}

struct Lecteur<'a> {
    octets: &'a [u8],
    texte: &'a str,
    pos: usize,
}

impl Lecteur<'_> {
    fn erreur<T>(&self, message: &str) -> Result<T, String> {
        Err(format!("{message} (position {})", self.pos + 1))
    }

    fn blancs(&mut self) {
        while matches!(
            self.octets.get(self.pos),
            Some(b' ' | b'\t' | b'\n' | b'\r')
        ) {
            self.pos += 1;
        }
    }

    fn attendre(&mut self, mot: &str) -> Result<(), String> {
        if self.texte[self.pos..].starts_with(mot) {
            self.pos += mot.len();
            Ok(())
        } else {
            self.erreur(&format!("'{mot}' attendu"))
        }
    }

    fn valeur(&mut self, profondeur: usize) -> Result<Json, String> {
        if profondeur > PROFONDEUR_MAX {
            return self.erreur("imbrication JSON trop profonde");
        }
        match self.octets.get(self.pos) {
            None => self.erreur("valeur attendue"),
            Some(b'n') => self.attendre("null").map(|_| Json::Nul),
            Some(b't') => self.attendre("true").map(|_| Json::Booleen(true)),
            Some(b'f') => self.attendre("false").map(|_| Json::Booleen(false)),
            Some(b'"') => self.chaine().map(Json::Chaine),
            Some(b'[') => {
                self.pos += 1;
                let mut elements = Vec::new();
                self.blancs();
                if self.octets.get(self.pos) == Some(&b']') {
                    self.pos += 1;
                    return Ok(Json::Tableau(elements));
                }
                loop {
                    self.blancs();
                    elements.push(self.valeur(profondeur + 1)?);
                    self.blancs();
                    match self.octets.get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(b']') => {
                            self.pos += 1;
                            return Ok(Json::Tableau(elements));
                        }
                        _ => return self.erreur("',' ou ']' attendu"),
                    }
                }
            }
            Some(b'{') => {
                self.pos += 1;
                let mut champs = Vec::new();
                self.blancs();
                if self.octets.get(self.pos) == Some(&b'}') {
                    self.pos += 1;
                    return Ok(Json::Objet(champs));
                }
                loop {
                    self.blancs();
                    if self.octets.get(self.pos) != Some(&b'"') {
                        return self.erreur("nom de champ attendu");
                    }
                    let cle = self.chaine()?;
                    self.blancs();
                    if self.octets.get(self.pos) != Some(&b':') {
                        return self.erreur("':' attendu");
                    }
                    self.pos += 1;
                    self.blancs();
                    let valeur = self.valeur(profondeur + 1)?;
                    champs.push((cle, valeur));
                    self.blancs();
                    match self.octets.get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            return Ok(Json::Objet(champs));
                        }
                        _ => return self.erreur("',' ou '}' attendu"),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.nombre(),
            Some(_) => self.erreur("valeur JSON invalide"),
        }
    }

    fn nombre(&mut self) -> Result<Json, String> {
        let debut = self.pos;
        let mut reel = false;
        if self.octets.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        while let Some(&o) = self.octets.get(self.pos) {
            match o {
                b'0'..=b'9' => {}
                b'.' | b'e' | b'E' | b'+' | b'-' => reel = true,
                _ => break,
            }
            self.pos += 1;
        }
        let texte = &self.texte[debut..self.pos];
        if !reel && let Ok(n) = texte.parse::<i64>() {
            return Ok(Json::Entier(n));
        }
        match texte.parse::<f64>() {
            Ok(r) => Ok(Json::Reel(r)),
            Err(_) => self.erreur("nombre invalide"),
        }
    }

    fn hexa4(&mut self) -> Result<u32, String> {
        let fin = self.pos + 4;
        let Some(texte) = self.texte.get(self.pos..fin) else {
            return self.erreur("sequence \\u incomplete");
        };
        let code =
            u32::from_str_radix(texte, 16).or_else(|_| self.erreur("sequence \\u invalide"))?;
        self.pos = fin;
        Ok(code)
    }

    fn chaine(&mut self) -> Result<String, String> {
        self.pos += 1; // guillemet ouvrant
        let mut resultat = String::new();
        loop {
            let Some(c) = self.texte[self.pos..].chars().next() else {
                return self.erreur("chaine non terminee");
            };
            self.pos += c.len_utf8();
            match c {
                '"' => return Ok(resultat),
                '\\' => {
                    let Some(e) = self.octets.get(self.pos).copied() else {
                        return self.erreur("echappement incomplet");
                    };
                    self.pos += 1;
                    match e {
                        b'"' => resultat.push('"'),
                        b'\\' => resultat.push('\\'),
                        b'/' => resultat.push('/'),
                        b'b' => resultat.push('\u{8}'),
                        b'f' => resultat.push('\u{c}'),
                        b'n' => resultat.push('\n'),
                        b'r' => resultat.push('\r'),
                        b't' => resultat.push('\t'),
                        b'u' => {
                            let mut code = self.hexa4()?;
                            if (0xD800..0xDC00).contains(&code) {
                                self.attendre("\\u")?;
                                let bas = self.hexa4()?;
                                code = 0x10000
                                    + ((code - 0xD800) << 10)
                                    + (bas.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            match char::from_u32(code) {
                                Some(c) => resultat.push(c),
                                None => return self.erreur("caractere \\u invalide"),
                            }
                        }
                        _ => return self.erreur("echappement inconnu"),
                    }
                }
                c if (c as u32) < 0x20 => {
                    return self.erreur("caractere de controle dans une chaine");
                }
                c => resultat.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour() {
        let texte = r#"{"t":0.002,"ev":"reaction","n":1,"in":[3,1],"out":[{"id":6,"v":-9}],"x":null,"b":true,"s":"a\"b\\c\n\u00e9"}"#;
        let json = Json::analyser(texte).unwrap();
        assert_eq!(json.champ("n").and_then(Json::comme_entier), Some(1));
        assert_eq!(json.champ("t").and_then(Json::comme_reel), Some(0.002));
        assert_eq!(
            json.champ("s").and_then(Json::comme_chaine),
            Some("a\"b\\c\n\u{e9}")
        );
        let reecrit = json.to_string();
        assert_eq!(Json::analyser(&reecrit).unwrap(), json);
    }

    #[test]
    fn entiers_exacts_et_erreurs() {
        assert_eq!(
            Json::analyser("9007199254740993").unwrap(),
            Json::Entier(9_007_199_254_740_993)
        );
        assert_eq!(Json::analyser("-1.5e3").unwrap(), Json::Reel(-1500.0));
        assert!(Json::analyser("{\"a\":}").is_err());
        assert!(Json::analyser("[1,2").is_err());
        assert!(Json::analyser("\"abc").is_err());
        assert!(Json::analyser("{} x").is_err());
        assert!(Json::analyser(&"[".repeat(1000)).is_err());
        assert_eq!(
            Json::analyser("\"\\ud83d\\ude00\"").unwrap(),
            Json::Chaine("\u{1F600}".into())
        );
    }
}
