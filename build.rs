use std::io::Write;

use icu_locale::{LanguageIdentifier, LocaleExpander};
use serde::Deserialize;

trait ToCode {
    fn to_code(&self) -> String;
}

#[derive(Debug, PartialEq)]
enum SubTagRule {
    Str(String),
    Var(String),
    VarExclude(String),
    All,
}

impl From<&'_ str> for SubTagRule {
    fn from(s: &'_ str) -> Self {
        if s == "*" {
            Self::All
        } else if let Some(name) = s.strip_prefix("$!") {
            Self::VarExclude(name.to_string())
        } else if let Some(name) = s.strip_prefix('$') {
            Self::Var(name.to_string())
        } else {
            Self::Str(s.to_string())
        }
    }
}

impl ToCode for SubTagRule {
    fn to_code(&self) -> String {
        match self {
            SubTagRule::Str(s) => format!("SubTagRule::Str({:?})", s),
            SubTagRule::Var(name) => format!("SubTagRule::Var({:?})", name),
            SubTagRule::VarExclude(name) => format!("SubTagRule::VarExclude({:?})", name),
            SubTagRule::All => "SubTagRule::All".to_string(),
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(from = "String")]
struct LanguageIdentifierRule {
    pub language: SubTagRule,
    pub script: Option<SubTagRule>,
    pub region: Option<SubTagRule>,
}

impl From<&'_ str> for LanguageIdentifierRule {
    fn from(s: &'_ str) -> Self {
        let mut parts = s.split('_');
        let language = parts.next().unwrap().into();
        let script = parts.next().map(|s| s.into());
        let region = parts.next().map(|s| s.into());
        Self {
            language,
            script,
            region,
        }
    }
}

impl From<String> for LanguageIdentifierRule {
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}

impl ToCode for LanguageIdentifierRule {
    fn to_code(&self) -> String {
        let script_code = match &self.script {
            Some(script) => format!("Some({})", script.to_code()),
            None => "None".to_string(),
        };
        let region_code = match &self.region {
            Some(region) => format!("Some({})", region.to_code()),
            None => "None".to_string(),
        };
        format!(
            "LanguageIdentifierRule {{ language: {}, script: {}, region: {} }}",
            self.language.to_code(),
            script_code,
            region_code
        )
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct ParadigmLocales {
    #[serde(rename = "@locales")]
    pub locales: String,
}

impl ToCode for ParadigmLocales {
    fn to_code(&self) -> String {
        let expander = LocaleExpander::new_extended();
        let arr = self
            .locales
            .split_whitespace()
            .map(|s| {
                let mut langid: LanguageIdentifier = s.parse().unwrap();
                expander.maximize(&mut langid);
                format!("icu_locale::langid!(\"{:?}\")", langid)
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("ParadigmLocales {{ locales: &[{}] }}", arr)
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct MatchVariable {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@value")]
    pub value: String,
}

impl ToCode for MatchVariable {
    fn to_code(&self) -> String {
        assert!(
            self.id.starts_with('$'),
            "MatchVariable id must start with '$'"
        );
        let value = self
            .value
            .split("+")
            .map(|s| format!("{:?}", s))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "MatchVariable {{ id: {:?}, value: &[{}] }}",
            &self.id[1..],
            value
        )
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct LanguageMatch {
    #[serde(rename = "@desired")]
    pub desired: LanguageIdentifierRule,
    #[serde(rename = "@supported")]
    pub supported: LanguageIdentifierRule,
    #[serde(rename = "@distance")]
    pub distance: u16,
    #[serde(default, rename = "@oneway")]
    pub oneway: bool,
}

impl ToCode for LanguageMatch {
    fn to_code(&self) -> String {
        format!(
            "LanguageMatch {{ desired: {}, supported: {}, distance: {}, oneway: {} }}",
            self.desired.to_code(),
            self.supported.to_code(),
            self.distance,
            self.oneway
        )
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct LanguageMatches {
    pub paradigm_locales: ParadigmLocales,
    pub match_variable: Vec<MatchVariable>,
    pub language_match: Vec<LanguageMatch>,
}

impl ToCode for LanguageMatches {
    fn to_code(&self) -> String {
        let paradigm_locales_code = self.paradigm_locales.to_code();
        let match_variable_code = format!(
            "&[{}]",
            self.match_variable
                .iter()
                .map(|mv| mv.to_code())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let language_match_code = format!(
            "&[{}]",
            self.language_match
                .iter()
                .map(|lm| lm.to_code())
                .collect::<Vec<_>>()
                .join(", ")
        );
        format!(
            "LanguageMatches {{ paradigm_locales: {}, match_variable: {}, language_match: {} }}",
            paradigm_locales_code, match_variable_code, language_match_code
        )
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct LanguageMatching {
    pub language_matches: LanguageMatches,
}

impl ToCode for LanguageMatching {
    fn to_code(&self) -> String {
        let language_matches_code = self.language_matches.to_code();
        format!(
            "LanguageMatching {{ language_matches: {} }}",
            language_matches_code
        )
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SupplementalData {
    pub language_matching: LanguageMatching,
}

impl ToCode for SupplementalData {
    fn to_code(&self) -> String {
        let language_matching_code = self.language_matching.to_code();
        format!(
            "SupplementalData {{ language_matching: {} }}",
            language_matching_code
        )
    }
}

const LANGUAGE_INFO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/languageInfo.xml"
));

fn main() {
    println!("cargo:rerun-if-changed=data/languageInfo.xml");

    let data: SupplementalData = quick_xml::de::from_str(LANGUAGE_INFO).unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let mut generated =
        std::fs::File::create(std::path::Path::new(&out_dir).join("language_info.rs")).unwrap();
    generated
        .write_all(b"// This file is generated by the build script.\n")
        .unwrap();
    generated
        .write_all(b"static LANGUAGE_INFO: SupplementalData = ")
        .unwrap();
    generated.write_all(data.to_code().as_bytes()).unwrap();
    generated.write_all(b";").unwrap();
}
