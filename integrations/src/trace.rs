//! Format de trace avec Serde (`serde_json`).
//!
//! Point d'integration : `chem::trace::FormatTrace`.
//!
//! La structure des evenements reste definie a un seul endroit : la
//! bibliotheque les convertit en valeurs `chem::trace::json::Json`
//! (`Evenement::vers_json`, `Evenement::depuis_json`, voir
//! docs/format-trace.md). Ce module ne remplace que le codage texte de ces
//! valeurs : `Serialize` et `Deserialize` sont implementes pour elles et
//! `serde_json` ecrit et lit les lignes.
//!
//! Les lignes produites sont identiques a celles de la bibliotheque, octet
//! pour octet :
//!
//! - les reels sont ecrits au format `Display` de Rust (`0.5`, `2` et non
//!   `2.0`) par un `serde_json::ser::Formatter` dedie ;
//! - les caracteres de controle sont echappes en `\u00XX` (Serde ecrirait
//!   `\b` et `\f` pour deux d'entre eux) ;
//! - les entiers restent exacts dans les deux sens (pas de passage par
//!   `f64`) et l'ordre des champs est conserve.

use chem::trace::json::Json;
use chem::trace::{Evenement, FormatTrace};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::ser::{CharEscape, Formatter};
use std::fmt;
use std::io;

/// Format de trace Serde.
#[derive(Debug, Default, Clone, Copy)]
pub struct FormatSerde;

impl FormatTrace for FormatSerde {
    fn nom(&self) -> &'static str {
        "serde_json"
    }

    fn ecrire(&self, evenement: &Evenement, sortie: &mut Vec<u8>) -> io::Result<()> {
        let json = evenement.vers_json();
        {
            let mut serialiseur =
                serde_json::Serializer::with_formatter(&mut *sortie, FormatCompact);
            Ecriture(&json)
                .serialize(&mut serialiseur)
                .map_err(io::Error::other)?;
        }
        sortie.push(b'\n');
        Ok(())
    }

    fn lire(&self, ligne: &str) -> Result<Evenement, String> {
        let Lecture(json) = serde_json::from_str::<Lecture>(ligne)
            .map_err(|erreur| format!("JSON invalide : {erreur}"))?;
        Evenement::depuis_json(&json)
    }
}

/// Ecriture d'une valeur JSON de la bibliotheque par Serde.
struct Ecriture<'a>(&'a Json);

impl Serialize for Ecriture<'_> {
    fn serialize<S: Serializer>(&self, serialiseur: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Json::Nul => serialiseur.serialize_unit(),
            Json::Booleen(b) => serialiseur.serialize_bool(*b),
            Json::Entier(n) => serialiseur.serialize_i64(*n),
            Json::Reel(r) => serialiseur.serialize_f64(*r),
            Json::Chaine(texte) => serialiseur.serialize_str(texte),
            Json::Tableau(elements) => {
                let mut tableau = serialiseur.serialize_seq(Some(elements.len()))?;
                for element in elements {
                    tableau.serialize_element(&Ecriture(element))?;
                }
                tableau.end()
            }
            Json::Objet(champs) => {
                let mut objet = serialiseur.serialize_map(Some(champs.len()))?;
                for (cle, valeur) in champs {
                    objet.serialize_entry(cle, &Ecriture(valeur))?;
                }
                objet.end()
            }
        }
    }
}

/// Valeur JSON de la bibliotheque lue par Serde.
struct Lecture(Json);

impl<'de> Deserialize<'de> for Lecture {
    fn deserialize<D: Deserializer<'de>>(deserialiseur: D) -> Result<Self, D::Error> {
        deserialiseur.deserialize_any(Visiteur)
    }
}

/// Construit une valeur `Json` a partir des appels de Serde.
struct Visiteur;

impl<'de> Visitor<'de> for Visiteur {
    type Value = Lecture;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("une valeur JSON")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Lecture, E> {
        Ok(Lecture(Json::Nul))
    }

    fn visit_bool<E: de::Error>(self, valeur: bool) -> Result<Lecture, E> {
        Ok(Lecture(Json::Booleen(valeur)))
    }

    fn visit_i64<E: de::Error>(self, valeur: i64) -> Result<Lecture, E> {
        Ok(Lecture(Json::Entier(valeur)))
    }

    fn visit_u64<E: de::Error>(self, valeur: u64) -> Result<Lecture, E> {
        // Comme la bibliotheque : au-dela de i64, le nombre devient un reel.
        Ok(Lecture(match i64::try_from(valeur) {
            Ok(n) => Json::Entier(n),
            Err(_) => Json::Reel(valeur as f64),
        }))
    }

    fn visit_f64<E: de::Error>(self, valeur: f64) -> Result<Lecture, E> {
        Ok(Lecture(Json::Reel(valeur)))
    }

    fn visit_str<E: de::Error>(self, valeur: &str) -> Result<Lecture, E> {
        Ok(Lecture(Json::Chaine(valeur.to_string())))
    }

    fn visit_string<E: de::Error>(self, valeur: String) -> Result<Lecture, E> {
        Ok(Lecture(Json::Chaine(valeur)))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut suite: A) -> Result<Lecture, A::Error> {
        let mut elements = Vec::new();
        while let Some(Lecture(element)) = suite.next_element::<Lecture>()? {
            elements.push(element);
        }
        Ok(Lecture(Json::Tableau(elements)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut champs: A) -> Result<Lecture, A::Error> {
        let mut objet = Vec::new();
        while let Some((cle, Lecture(valeur))) = champs.next_entry::<String, Lecture>()? {
            objet.push((cle, valeur));
        }
        Ok(Lecture(Json::Objet(objet)))
    }
}

/// Sortie compacte de `serde_json` alignee sur celle de la bibliotheque.
struct FormatCompact;

impl Formatter for FormatCompact {
    fn write_f64<W: ?Sized + io::Write>(&mut self, writer: &mut W, valeur: f64) -> io::Result<()> {
        write!(writer, "{valeur}")
    }

    fn write_char_escape<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        echappement: CharEscape,
    ) -> io::Result<()> {
        let sequence: &[u8] = match echappement {
            CharEscape::Quote => b"\\\"",
            CharEscape::ReverseSolidus => b"\\\\",
            CharEscape::Solidus => b"\\/",
            CharEscape::LineFeed => b"\\n",
            CharEscape::CarriageReturn => b"\\r",
            CharEscape::Tab => b"\\t",
            CharEscape::Backspace => b"\\u0008",
            CharEscape::FormFeed => b"\\u000c",
            CharEscape::AsciiControl(octet) => return write!(writer, "\\u{octet:04x}"),
        };
        writer.write_all(sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chem::trace::FormatInterne;

    fn lignes(format: &dyn FormatTrace, evenement: &Evenement) -> String {
        let mut sortie = Vec::new();
        format.ecrire(evenement, &mut sortie).unwrap();
        String::from_utf8(sortie).unwrap()
    }

    #[test]
    fn chaines_et_reels_comme_la_bibliotheque() {
        let evenements = [
            Evenement::Debut {
                graine: u64::MAX,
                strategie: "aleatoire".to_string(),
                workers: 4,
                programme: "a\u{8}b\u{c}c\"d\\e\nf\rg\th\u{1}i\u{1f}j/k\u{7f}l \u{e9} \u{2211}"
                    .to_string(),
            },
            Evenement::Fin {
                t: 2.0,
                reactions: 0,
                inerte: true,
                motif: "inertie".to_string(),
            },
            Evenement::Fin {
                t: 0.000001,
                reactions: 12,
                inerte: false,
                motif: "delai".to_string(),
            },
            Evenement::Fin {
                t: 123456.5,
                reactions: u64::MAX >> 1,
                inerte: false,
                motif: "max-etapes".to_string(),
            },
        ];
        for evenement in &evenements {
            let interne = lignes(&FormatInterne, evenement);
            assert_eq!(lignes(&FormatSerde, evenement), interne);
            let ligne = interne.trim_end_matches('\n');
            assert_eq!(FormatSerde.lire(ligne), FormatInterne.lire(ligne));
        }
    }

    #[test]
    fn lignes_invalides_refusees() {
        for ligne in [
            "",
            "{",
            "[1,2",
            "{\"ev\":\"inconnu\"}",
            "{\"t\":1} x",
            "nul",
        ] {
            assert!(FormatSerde.lire(ligne).is_err(), "{ligne}");
            assert!(FormatInterne.lire(ligne).is_err(), "{ligne}");
        }
    }
}
