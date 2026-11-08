//! Evenements d'execution et leur forme JSON (une ligne par evenement).
//!
//! Le format est decrit dans docs/format-trace.md ; il etend l'annexe C du
//! cahier des charges (champs `in_sol`, evenements `debut` et `destruction`).
//! La version 2 ajoute les regles a usage unique (champ `retiree`) et les
//! injections asynchrones (evenements `envoi` et `reception`) ; une trace de
//! version 1 reste une trace valide de version 2.

use super::json::Json;
use crate::evaluateur::Valeur;
use crate::solution::{IdMolecule, IdSolution, Molecule};
use std::sync::Arc;

/// Version du format de trace.
pub const VERSION_FORMAT: i64 = 2;

/// Evenement emis par le moteur.
#[derive(Debug, Clone, PartialEq)]
pub enum Evenement {
    /// Parametres de l'execution (premier evenement).
    Debut {
        /// Graine du generateur pseudo-aleatoire.
        graine: u64,
        /// `aleatoire` ou `ordonnee`.
        strategie: String,
        /// Nombre de workers.
        workers: usize,
        /// Nom du programme.
        programme: String,
    },
    /// Etat initial d'une solution (emis en ordre prefixe).
    Init {
        /// Instant (secondes depuis le debut).
        t: f64,
        /// Solution decrite.
        sol: IdSolution,
        /// Solution parente (`None` pour la racine).
        parent: Option<IdSolution>,
        /// Regles actives.
        regles: Vec<String>,
        /// Molecules directes.
        molecules: Vec<Molecule>,
    },
    /// Application d'une regle.
    Reaction {
        /// Instant.
        t: f64,
        /// Numero global de la reaction (a partir de 1).
        n: u64,
        /// Worker qui l'a realisee.
        w: usize,
        /// Solution ou la reaction a lieu.
        sol: IdSolution,
        /// Nom de la regle.
        regle: Arc<str>,
        /// Molecules consommees, dans l'ordre des motifs.
        entrees: Vec<IdMolecule>,
        /// Sous-solutions liees, dans l'ordre des motifs.
        entrees_sol: Vec<IdSolution>,
        /// Molecules produites dans `sol`.
        sorties: Vec<Molecule>,
        /// Vrai si la regle, a usage unique, est retiree de `sol` (EF-D5).
        retiree: bool,
    },
    /// Dissolution d'une sous-solution liee par la reaction `n`.
    Dissolution {
        /// Instant.
        t: f64,
        /// Numero de la reaction.
        n: u64,
        /// Solution parente (qui recoit le contenu).
        sol: IdSolution,
        /// Sous-solution dissoute.
        cible: IdSolution,
        /// Molecules liberees.
        molecules: Vec<IdMolecule>,
        /// Sous-solutions liberees.
        solutions: Vec<IdSolution>,
    },
    /// Injection de molecules dans une sous-solution liee par la reaction `n`.
    Injection {
        /// Instant.
        t: f64,
        /// Numero de la reaction.
        n: u64,
        /// Solution parente.
        sol: IdSolution,
        /// Sous-solution qui recoit les molecules (reactivee).
        cible: IdSolution,
        /// Molecules injectees.
        sorties: Vec<Molecule>,
    },
    /// Injection asynchrone par la reaction `n` (EF-D6) : les molecules sont
    /// deposees dans la boite aux lettres de `cible`, sous-solution directe
    /// de `sol` (inerte ou en cours d'execution), qui les recevra plus tard.
    Envoi {
        /// Instant.
        t: f64,
        /// Numero de la reaction.
        n: u64,
        /// Solution ou la reaction a lieu (parente de `cible`).
        sol: IdSolution,
        /// Sous-solution destinataire.
        cible: IdSolution,
        /// Molecules envoyees (nouvelles molecules, en transit).
        sorties: Vec<Molecule>,
    },
    /// Releve du courrier : les molecules en transit vers `sol` rejoignent
    /// son multi-ensemble.
    Reception {
        /// Instant.
        t: f64,
        /// Solution destinataire.
        sol: IdSolution,
        /// Molecules recues (annoncees par des evenements `envoi`).
        molecules: Vec<IdMolecule>,
    },
    /// Sous-solution liee et non reproduite : detruite avec son contenu.
    Destruction {
        /// Instant.
        t: f64,
        /// Numero de la reaction.
        n: u64,
        /// Solution parente.
        sol: IdSolution,
        /// Sous-solution detruite.
        cible: IdSolution,
    },
    /// Une solution vient d'atteindre l'inertie.
    Inertie {
        /// Instant.
        t: f64,
        /// Solution inerte.
        sol: IdSolution,
    },
    /// Fin de l'execution (dernier evenement).
    Fin {
        /// Instant.
        t: f64,
        /// Nombre total de reactions.
        reactions: u64,
        /// Vrai si la solution principale est inerte.
        inerte: bool,
        /// `inertie`, `max-etapes`, `delai`, `max-molecules`, `interruption`, `erreur`.
        motif: String,
    },
}

fn temps(t: f64) -> Json {
    Json::Reel((t * 1e6).round() / 1e6)
}

fn id_sol(id: IdSolution) -> Json {
    Json::Chaine(id.to_string())
}

fn ids_sol(ids: &[IdSolution]) -> Json {
    Json::Tableau(ids.iter().copied().map(id_sol).collect())
}

fn ids_mol(ids: &[IdMolecule]) -> Json {
    Json::Tableau(ids.iter().map(|i| Json::Entier(i.0 as i64)).collect())
}

/// Valeur -> JSON (entier, booleen, chaine, tableau pour un n-uplet).
pub fn valeur_vers_json(v: &Valeur) -> Json {
    match v {
        Valeur::Entier(n) => Json::Entier(*n),
        Valeur::Booleen(b) => Json::Booleen(*b),
        Valeur::Chaine(s) => Json::Chaine(s.to_string()),
        Valeur::NUplet(es) => Json::Tableau(es.iter().map(valeur_vers_json).collect()),
    }
}

/// JSON -> Valeur.
pub fn valeur_depuis_json(j: &Json) -> Result<Valeur, String> {
    match j {
        Json::Entier(n) => Ok(Valeur::Entier(*n)),
        Json::Booleen(b) => Ok(Valeur::Booleen(*b)),
        Json::Chaine(s) => Ok(Valeur::chaine(s)),
        Json::Tableau(es) => Ok(Valeur::nuplet(
            es.iter()
                .map(valeur_depuis_json)
                .collect::<Result<_, _>>()?,
        )),
        autre => Err(format!("valeur de molecule invalide : {autre}")),
    }
}

fn molecules(ms: &[Molecule]) -> Json {
    Json::Tableau(
        ms.iter()
            .map(|m| {
                Json::objet(vec![
                    ("id", Json::Entier(m.id.0 as i64)),
                    ("v", valeur_vers_json(&m.valeur)),
                ])
            })
            .collect(),
    )
}

impl Evenement {
    /// Nom de l'evenement (champ `ev`).
    pub fn nom(&self) -> &'static str {
        match self {
            Evenement::Debut { .. } => "debut",
            Evenement::Init { .. } => "init",
            Evenement::Reaction { .. } => "reaction",
            Evenement::Dissolution { .. } => "dissolution",
            Evenement::Injection { .. } => "injection",
            Evenement::Envoi { .. } => "envoi",
            Evenement::Reception { .. } => "reception",
            Evenement::Destruction { .. } => "destruction",
            Evenement::Inertie { .. } => "inertie",
            Evenement::Fin { .. } => "fin",
        }
    }

    /// Forme JSON de l'evenement.
    pub fn vers_json(&self) -> Json {
        let ev = ("ev", Json::Chaine(self.nom().to_string()));
        match self {
            Evenement::Debut {
                graine,
                strategie,
                workers,
                programme,
            } => Json::objet(vec![
                ("t", Json::Reel(0.0)),
                ev,
                ("version", Json::Entier(VERSION_FORMAT)),
                // la graine est ecrite en texte : un u64 ne tient pas toujours dans un i64
                ("graine", Json::Chaine(graine.to_string())),
                ("strategie", Json::Chaine(strategie.clone())),
                ("workers", Json::Entier(*workers as i64)),
                ("programme", Json::Chaine(programme.clone())),
            ]),
            Evenement::Init {
                t,
                sol,
                parent,
                regles,
                molecules: ms,
            } => Json::objet(vec![
                ("t", temps(*t)),
                ev,
                ("sol", id_sol(*sol)),
                ("parent", parent.map_or(Json::Nul, id_sol)),
                (
                    "regles",
                    Json::Tableau(regles.iter().map(|r| Json::Chaine(r.clone())).collect()),
                ),
                ("mol", molecules(ms)),
            ]),
            Evenement::Reaction {
                t,
                n,
                w,
                sol,
                regle,
                entrees,
                entrees_sol,
                sorties,
                retiree,
            } => {
                let mut champs = vec![
                    ("t", temps(*t)),
                    ev,
                    ("n", Json::Entier(*n as i64)),
                    ("w", Json::Entier(*w as i64)),
                    ("sol", id_sol(*sol)),
                    ("regle", Json::Chaine(regle.to_string())),
                    ("in", ids_mol(entrees)),
                ];
                if !entrees_sol.is_empty() {
                    champs.push(("in_sol", ids_sol(entrees_sol)));
                }
                champs.push(("out", molecules(sorties)));
                if *retiree {
                    champs.push(("retiree", Json::Booleen(true)));
                }
                Json::objet(champs)
            }
            Evenement::Dissolution {
                t,
                n,
                sol,
                cible,
                molecules: ms,
                solutions,
            } => Json::objet(vec![
                ("t", temps(*t)),
                ev,
                ("n", Json::Entier(*n as i64)),
                ("sol", id_sol(*sol)),
                ("cible", id_sol(*cible)),
                ("mol", ids_mol(ms)),
                ("sous", ids_sol(solutions)),
            ]),
            Evenement::Injection {
                t,
                n,
                sol,
                cible,
                sorties,
            } => Json::objet(vec![
                ("t", temps(*t)),
                ev,
                ("n", Json::Entier(*n as i64)),
                ("sol", id_sol(*sol)),
                ("cible", id_sol(*cible)),
                ("out", molecules(sorties)),
            ]),
            Evenement::Envoi {
                t,
                n,
                sol,
                cible,
                sorties,
            } => Json::objet(vec![
                ("t", temps(*t)),
                ev,
                ("n", Json::Entier(*n as i64)),
                ("sol", id_sol(*sol)),
                ("cible", id_sol(*cible)),
                ("out", molecules(sorties)),
            ]),
            Evenement::Reception {
                t,
                sol,
                molecules: ms,
            } => Json::objet(vec![
                ("t", temps(*t)),
                ev,
                ("sol", id_sol(*sol)),
                ("mol", ids_mol(ms)),
            ]),
            Evenement::Destruction { t, n, sol, cible } => Json::objet(vec![
                ("t", temps(*t)),
                ev,
                ("n", Json::Entier(*n as i64)),
                ("sol", id_sol(*sol)),
                ("cible", id_sol(*cible)),
            ]),
            Evenement::Inertie { t, sol } => {
                Json::objet(vec![("t", temps(*t)), ev, ("sol", id_sol(*sol))])
            }
            Evenement::Fin {
                t,
                reactions,
                inerte,
                motif,
            } => Json::objet(vec![
                ("t", temps(*t)),
                ev,
                ("reactions", Json::Entier(*reactions as i64)),
                ("inerte", Json::Booleen(*inerte)),
                ("motif", Json::Chaine(motif.clone())),
            ]),
        }
    }

    /// Lecture d'un evenement depuis sa forme JSON.
    pub fn depuis_json(json: &Json) -> Result<Evenement, String> {
        let lecture = Lecture(json);
        let nom = lecture.chaine("ev")?;
        let t = || lecture.reel_ou("t", 0.0);
        Ok(match nom {
            "debut" => Evenement::Debut {
                graine: match json.champ("graine") {
                    Some(Json::Chaine(s)) => {
                        s.parse().map_err(|_| "graine invalide".to_string())?
                    }
                    Some(j) => j.comme_u64().ok_or("graine invalide")?,
                    None => 0,
                },
                strategie: lecture.chaine("strategie").unwrap_or("").to_string(),
                workers: lecture.u64_ou("workers", 1) as usize,
                programme: lecture.chaine("programme").unwrap_or("").to_string(),
            },
            "init" => Evenement::Init {
                t: t(),
                sol: lecture.solution("sol")?,
                parent: match json.champ("parent") {
                    None | Some(Json::Nul) => None,
                    Some(_) => Some(lecture.solution("parent")?),
                },
                regles: lecture
                    .tableau_ou_vide("regles")?
                    .iter()
                    .map(|r| {
                        r.comme_chaine()
                            .map(str::to_string)
                            .ok_or("nom de regle invalide")
                    })
                    .collect::<Result<_, _>>()?,
                molecules: lecture.molecules("mol")?,
            },
            "reaction" => Evenement::Reaction {
                t: t(),
                n: lecture.u64("n")?,
                w: lecture.u64_ou("w", 0) as usize,
                sol: lecture.solution("sol")?,
                regle: Arc::from(lecture.chaine("regle")?),
                entrees: lecture.ids_molecules("in")?,
                entrees_sol: lecture.ids_solutions("in_sol")?,
                sorties: lecture.molecules("out")?,
                retiree: lecture.booleen_ou("retiree", false)?,
            },
            "dissolution" => Evenement::Dissolution {
                t: t(),
                n: lecture.u64("n")?,
                sol: lecture.solution("sol")?,
                cible: lecture.solution("cible")?,
                molecules: lecture.ids_molecules("mol")?,
                solutions: lecture.ids_solutions("sous")?,
            },
            "injection" => Evenement::Injection {
                t: t(),
                n: lecture.u64("n")?,
                sol: lecture.solution("sol")?,
                cible: lecture.solution("cible")?,
                sorties: lecture.molecules("out")?,
            },
            "envoi" => Evenement::Envoi {
                t: t(),
                n: lecture.u64("n")?,
                sol: lecture.solution("sol")?,
                cible: lecture.solution("cible")?,
                sorties: lecture.molecules("out")?,
            },
            "reception" => Evenement::Reception {
                t: t(),
                sol: lecture.solution("sol")?,
                molecules: lecture.ids_molecules("mol")?,
            },
            "destruction" => Evenement::Destruction {
                t: t(),
                n: lecture.u64("n")?,
                sol: lecture.solution("sol")?,
                cible: lecture.solution("cible")?,
            },
            "inertie" => Evenement::Inertie {
                t: t(),
                sol: lecture.solution("sol")?,
            },
            "fin" => Evenement::Fin {
                t: t(),
                reactions: lecture.u64_ou("reactions", 0),
                inerte: json
                    .champ("inerte")
                    .and_then(Json::comme_booleen)
                    .unwrap_or(false),
                motif: lecture.chaine("motif").unwrap_or("").to_string(),
            },
            autre => return Err(format!("evenement inconnu '{autre}'")),
        })
    }

    /// Lecture d'une ligne JSON.
    pub fn depuis_ligne(ligne: &str) -> Result<Evenement, String> {
        Self::depuis_json(&Json::analyser(ligne)?)
    }

    /// Ligne JSON (sans fin de ligne).
    pub fn vers_ligne(&self) -> String {
        self.vers_json().to_string()
    }
}

struct Lecture<'a>(&'a Json);

impl<'a> Lecture<'a> {
    fn champ(&self, nom: &str) -> Result<&'a Json, String> {
        self.0
            .champ(nom)
            .ok_or_else(|| format!("champ '{nom}' manquant"))
    }

    fn chaine(&self, nom: &str) -> Result<&'a str, String> {
        self.champ(nom)?
            .comme_chaine()
            .ok_or_else(|| format!("champ '{nom}' : chaine attendue"))
    }

    fn u64(&self, nom: &str) -> Result<u64, String> {
        self.champ(nom)?
            .comme_u64()
            .ok_or_else(|| format!("champ '{nom}' : entier positif attendu"))
    }

    fn u64_ou(&self, nom: &str, defaut: u64) -> u64 {
        self.0
            .champ(nom)
            .and_then(Json::comme_u64)
            .unwrap_or(defaut)
    }

    fn booleen_ou(&self, nom: &str, defaut: bool) -> Result<bool, String> {
        match self.0.champ(nom) {
            None => Ok(defaut),
            Some(j) => j
                .comme_booleen()
                .ok_or_else(|| format!("champ '{nom}' : booleen attendu")),
        }
    }

    fn reel_ou(&self, nom: &str, defaut: f64) -> f64 {
        self.0
            .champ(nom)
            .and_then(Json::comme_reel)
            .unwrap_or(defaut)
    }

    fn solution(&self, nom: &str) -> Result<IdSolution, String> {
        let texte = self.chaine(nom)?;
        IdSolution::depuis_texte(texte)
            .ok_or_else(|| format!("champ '{nom}' : solution 'sN' attendue"))
    }

    fn tableau_ou_vide(&self, nom: &str) -> Result<&'a [Json], String> {
        match self.0.champ(nom) {
            None => Ok(&[]),
            Some(j) => j
                .comme_tableau()
                .ok_or_else(|| format!("champ '{nom}' : tableau attendu")),
        }
    }

    fn ids_molecules(&self, nom: &str) -> Result<Vec<IdMolecule>, String> {
        self.tableau_ou_vide(nom)?
            .iter()
            .map(|j| {
                j.comme_u64()
                    .map(IdMolecule)
                    .ok_or_else(|| format!("champ '{nom}' : identifiant de molecule invalide"))
            })
            .collect()
    }

    fn ids_solutions(&self, nom: &str) -> Result<Vec<IdSolution>, String> {
        self.tableau_ou_vide(nom)?
            .iter()
            .map(|j| {
                j.comme_chaine()
                    .and_then(IdSolution::depuis_texte)
                    .ok_or_else(|| format!("champ '{nom}' : identifiant de solution invalide"))
            })
            .collect()
    }

    fn molecules(&self, nom: &str) -> Result<Vec<Molecule>, String> {
        self.tableau_ou_vide(nom)?
            .iter()
            .map(|j| {
                let id = j
                    .champ("id")
                    .and_then(Json::comme_u64)
                    .ok_or_else(|| format!("champ '{nom}' : 'id' manquant"))?;
                let v = j
                    .champ("v")
                    .ok_or_else(|| format!("champ '{nom}' : 'v' manquant"))?;
                Ok(Molecule {
                    id: IdMolecule(id),
                    valeur: valeur_depuis_json(v)?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: u64, v: i64) -> Molecule {
        Molecule {
            id: IdMolecule(id),
            valeur: Valeur::Entier(v),
        }
    }

    #[test]
    fn format_annexe_c() {
        let e = Evenement::Reaction {
            t: 0.002,
            n: 1,
            w: 0,
            sol: IdSolution(0),
            regle: Arc::from("max"),
            entrees: vec![IdMolecule(3), IdMolecule(1)],
            entrees_sol: vec![],
            sorties: vec![m(6, 9)],
            retiree: false,
        };
        assert_eq!(
            e.vers_ligne(),
            r#"{"t":0.002,"ev":"reaction","n":1,"w":0,"sol":"s0","regle":"max","in":[3,1],"out":[{"id":6,"v":9}]}"#
        );
        assert_eq!(Evenement::depuis_ligne(&e.vers_ligne()).unwrap(), e);
    }

    #[test]
    fn format_version_2() {
        let r = Evenement::Reaction {
            t: 0.0,
            n: 1,
            w: 0,
            sol: IdSolution(0),
            regle: Arc::from("amorcer"),
            entrees: vec![],
            entrees_sol: vec![],
            sorties: vec![],
            retiree: true,
        };
        assert_eq!(
            r.vers_ligne(),
            r#"{"t":0,"ev":"reaction","n":1,"w":0,"sol":"s0","regle":"amorcer","in":[],"out":[],"retiree":true}"#
        );
        let e = Evenement::Envoi {
            t: 0.0,
            n: 1,
            sol: IdSolution(0),
            cible: IdSolution(1),
            sorties: vec![m(4, 50)],
        };
        assert_eq!(
            e.vers_ligne(),
            r#"{"t":0,"ev":"envoi","n":1,"sol":"s0","cible":"s1","out":[{"id":4,"v":50}]}"#
        );
        let r = Evenement::Reception {
            t: 0.0,
            sol: IdSolution(1),
            molecules: vec![IdMolecule(4)],
        };
        assert_eq!(
            r.vers_ligne(),
            r#"{"t":0,"ev":"reception","sol":"s1","mol":[4]}"#
        );
        // une trace de version 1 (sans 'retiree') reste lisible
        let v1 = Evenement::depuis_ligne(
            r#"{"t":0,"ev":"reaction","n":1,"w":0,"sol":"s0","regle":"max","in":[3,1],"out":[]}"#,
        )
        .unwrap();
        assert!(matches!(v1, Evenement::Reaction { retiree: false, .. }));
        assert!(
            Evenement::depuis_ligne(
                r#"{"t":0,"ev":"reaction","n":1,"sol":"s0","regle":"r","retiree":1}"#
            )
            .is_err()
        );
    }

    #[test]
    fn aller_retour_de_tous_les_evenements() {
        let tuple = Valeur::nuplet(vec![
            Valeur::Entier(1),
            Valeur::chaine("x"),
            Valeur::Booleen(false),
        ]);
        let evenements = vec![
            Evenement::Debut {
                graine: u64::MAX,
                strategie: "aleatoire".into(),
                workers: 4,
                programme: "b6.chem".into(),
            },
            Evenement::Init {
                t: 0.0,
                sol: IdSolution(1),
                parent: Some(IdSolution(0)),
                regles: vec!["max".into()],
                molecules: vec![
                    m(1, 4),
                    Molecule {
                        id: IdMolecule(2),
                        valeur: tuple,
                    },
                ],
            },
            Evenement::Reaction {
                t: 0.5,
                n: 7,
                w: 2,
                sol: IdSolution(0),
                regle: Arc::from("envoyer"),
                entrees: vec![IdMolecule(2)],
                entrees_sol: vec![IdSolution(1)],
                sorties: vec![],
                retiree: true,
            },
            Evenement::Injection {
                t: 0.5,
                n: 7,
                sol: IdSolution(0),
                cible: IdSolution(1),
                sorties: vec![m(9, 10)],
            },
            Evenement::Envoi {
                t: 0.55,
                n: 7,
                sol: IdSolution(0),
                cible: IdSolution(3),
                sorties: vec![m(10, 1), m(11, 2)],
            },
            Evenement::Reception {
                t: 0.56,
                sol: IdSolution(3),
                molecules: vec![IdMolecule(10), IdMolecule(11)],
            },
            Evenement::Dissolution {
                t: 0.6,
                n: 8,
                sol: IdSolution(0),
                cible: IdSolution(1),
                molecules: vec![IdMolecule(1)],
                solutions: vec![IdSolution(4)],
            },
            Evenement::Destruction {
                t: 0.7,
                n: 9,
                sol: IdSolution(0),
                cible: IdSolution(2),
            },
            Evenement::Inertie {
                t: 0.8,
                sol: IdSolution(1),
            },
            Evenement::Fin {
                t: 1.0,
                reactions: 9,
                inerte: true,
                motif: "inertie".into(),
            },
        ];
        for e in evenements {
            assert_eq!(Evenement::depuis_ligne(&e.vers_ligne()).unwrap(), e);
        }
    }

    #[test]
    fn erreurs_de_lecture() {
        assert!(Evenement::depuis_ligne("{}").is_err());
        assert!(Evenement::depuis_ligne(r#"{"ev":"autre"}"#).is_err());
        assert!(
            Evenement::depuis_ligne(r#"{"ev":"reaction","n":1,"sol":"x","regle":"r"}"#).is_err()
        );
        assert!(
            Evenement::depuis_ligne(r#"{"ev":"init","sol":"s0","mol":[{"id":1,"v":1.5}]}"#)
                .is_err()
        );
    }
}
