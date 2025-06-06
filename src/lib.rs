//! A language matcher with CLDR.
//!
//! The "sync" feature of `icu_provider` is enabled because we like Sync.

#![warn(missing_docs)]
#![deny(unsafe_code)]

use icu_locale::{LanguageIdentifier, LocaleExpander};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

trait Rule<T> {
    fn matches(self, tag: T, vars: &Variables) -> bool;
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

impl Rule<&'_ str> for &'_ SubTagRule {
    fn matches(self, tag: &str, vars: &Variables) -> bool {
        match self {
            SubTagRule::Str(s) => s == tag,
            SubTagRule::Var(key) => vars[key].contains(tag),
            SubTagRule::VarExclude(key) => !vars[key].contains(tag),
            SubTagRule::All => true,
        }
    }
}

impl Rule<Option<&'_ str>> for Option<&'_ SubTagRule> {
    fn matches(self, tag: Option<&str>, vars: &Variables) -> bool {
        match (self, tag) {
            (None, None) | (Some(SubTagRule::All), _) => true,
            (Some(s), Some(tag)) => s.matches(tag, vars),
            _ => false,
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

impl Rule<&'_ LanguageIdentifier> for &'_ LanguageIdentifierRule {
    fn matches(self, lang: &LanguageIdentifier, vars: &Variables) -> bool {
        self.language.matches(lang.language.as_str(), vars)
            && self
                .script
                .as_ref()
                .matches(lang.script.as_ref().map(|s| s.as_str()), vars)
            && self
                .region
                .as_ref()
                .matches(lang.region.as_ref().map(|s| s.as_str()), vars)
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct ParadigmLocales {
    #[serde(rename = "@locales")]
    pub locales: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct MatchVariable {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@value")]
    pub value: String,
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

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct LanguageMatches {
    pub paradigm_locales: ParadigmLocales,
    pub match_variable: Vec<MatchVariable>,
    pub language_match: Vec<LanguageMatch>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct LanguageMatching {
    pub language_matches: LanguageMatches,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SupplementalData {
    pub language_matching: LanguageMatching,
}

const LANGUAGE_INFO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/languageInfo.xml"
));

/// This is a language matcher.
/// The distance of two languages are calculated by the algorithm of [CLDR].
/// The value of distance is multiplied by 10, because we need to consider the paradigm locales.
///
/// [CLDR]: https://www.unicode.org/reports/tr35/tr35.html#EnhancedLanguageMatching
///
/// # Examples
///
/// ```
/// use icu_locale::langid;
/// use language_matcher::LanguageMatcher;
///
/// let matcher = LanguageMatcher::new();
/// assert_eq!(matcher.distance(langid!("zh-CN"), langid!("zh-Hans")), 0);
/// assert_eq!(matcher.distance(langid!("zh-HK"), langid!("zh-MO")), 40);
/// assert_eq!(matcher.distance(langid!("en-US"), langid!("en-GB")), 50);
/// assert_eq!(matcher.distance(langid!("en-US"), langid!("en-CA")), 39);
/// ```
///
/// With the distance, you can choose the nearst language from a set of languages:
///
/// ```
/// use icu_locale::langid;
/// use language_matcher::LanguageMatcher;
///
/// let matcher = LanguageMatcher::new();
/// let accepts = [
///     langid!("en"),
///     langid!("ja"),
///     langid!("zh-Hans"),
///     langid!("zh-Hant"),
/// ];
///
/// assert_eq!(matcher.matches(langid!("zh-CN"), &accepts),Some((&langid!("zh-Hans"), 0)));
/// ```
pub struct LanguageMatcher {
    paradigm: HashSet<LanguageIdentifier>,
    vars: Variables,
    rules: Vec<LanguageMatch>,
    expander: LocaleExpander,
}

type Variables = HashMap<String, HashSet<String>>;

impl From<SupplementalData> for LanguageMatcher {
    fn from(data: SupplementalData) -> Self {
        let expander = LocaleExpander::new_extended();

        let matches = data.language_matching.language_matches;

        let paradigm = matches
            .paradigm_locales
            .locales
            .split(' ')
            .map(|s| {
                let mut lang = s.parse().unwrap();
                expander.maximize(&mut lang);
                lang
            })
            .collect::<HashSet<_>>();
        let vars = matches
            .match_variable
            .into_iter()
            .map(|MatchVariable { id, value }| {
                debug_assert!(id.starts_with('$'));
                // TODO: we need to support '-' as well, but there's no '-' in the data.
                (
                    id[1..].to_string(),
                    value.split('+').map(|s| s.to_string()).collect(),
                )
            })
            .collect::<HashMap<_, _>>();
        Self {
            paradigm,
            vars,
            rules: matches.language_match,
            expander,
        }
    }
}

impl LanguageMatcher {
    /// Creates an instance of [`LanguageMatcher`].
    pub fn new() -> Self {
        let data: SupplementalData = quick_xml::de::from_str(LANGUAGE_INFO).unwrap();
        data.into()
    }

    /// Choose the nearst language of desired language from the supported language collection.
    /// Returns the chosen language and the distance.
    ///
    /// `None` will be returned if no language gives the distance less than 1000.
    /// That usually means no language matches the desired one.
    pub fn matches<'a>(
        &self,
        mut desired: LanguageIdentifier,
        supported: impl IntoIterator<Item = &'a LanguageIdentifier>,
    ) -> Option<(&'a LanguageIdentifier, u16)> {
        self.expander.maximize(&mut desired);
        supported
            .into_iter()
            .map(|s| {
                let mut max_s = s.clone();
                self.expander.maximize(&mut max_s);
                (s, self.distance_impl(desired.clone(), max_s))
            })
            .min_by_key(|(_, dis)| *dis)
            .filter(|(_, dis)| *dis < 1000)
    }

    /// Calculate the distance of the two language.
    /// Some rule in CLDR is one way. Be careful about the parameters order.
    ///
    /// The return value is multiplied by 10, and if only one is paradigm locale,
    /// the value is substructed by 1.
    pub fn distance(
        &self,
        mut desired: LanguageIdentifier,
        mut supported: LanguageIdentifier,
    ) -> u16 {
        self.expander.maximize(&mut desired);
        self.expander.maximize(&mut supported);
        self.distance_impl(desired, supported)
    }

    fn distance_impl(
        &self,
        mut desired: LanguageIdentifier,
        mut supported: LanguageIdentifier,
    ) -> u16 {
        debug_assert!(desired.region.is_some());
        debug_assert!(desired.script.is_some());
        debug_assert!(supported.region.is_some());
        debug_assert!(supported.script.is_some());

        let mut distance = 0;

        if desired.region != supported.region {
            distance += self.distance_match(&desired, &supported);
        }
        desired.region = None;
        supported.region = None;

        if desired.script != supported.script {
            distance += self.distance_match(&desired, &supported);
        }
        desired.script = None;
        supported.script = None;

        if desired.language != supported.language {
            distance += self.distance_match(&desired, &supported);
        }

        distance
    }

    fn distance_match(&self, desired: &LanguageIdentifier, supported: &LanguageIdentifier) -> u16 {
        for rule in &self.rules {
            let mut matches = rule.desired.matches(desired, &self.vars)
                && rule.supported.matches(supported, &self.vars);
            if !rule.oneway && !matches {
                matches = rule.supported.matches(desired, &self.vars)
                    && rule.desired.matches(supported, &self.vars);
            }
            if matches {
                let mut distance = rule.distance * 10;
                if self.is_paradigm(desired) ^ self.is_paradigm(supported) {
                    distance -= 1
                }
                return distance;
            }
        }
        unreachable!()
    }

    fn is_paradigm(&self, lang: &LanguageIdentifier) -> bool {
        self.paradigm.contains(lang)
    }
}

impl Default for LanguageMatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use crate::LanguageMatcher;
    use icu_locale::langid;

    #[test]
    fn distance() {
        let matcher = LanguageMatcher::new();

        assert_eq!(matcher.distance(langid!("zh-CN"), langid!("zh-Hans")), 0);
        assert_eq!(matcher.distance(langid!("zh-TW"), langid!("zh-Hant")), 0);
        assert_eq!(matcher.distance(langid!("zh-HK"), langid!("zh-MO")), 40);
        assert_eq!(matcher.distance(langid!("zh-HK"), langid!("zh-Hant")), 50);
    }

    #[test]
    fn matcher() {
        let matcher = LanguageMatcher::new();

        let accepts = [
            langid!("en"),
            langid!("ja"),
            langid!("zh-Hans"),
            langid!("zh-Hant"),
        ];
        assert_eq!(
            matcher.matches(langid!("zh-CN"), &accepts),
            Some((&langid!("zh-Hans"), 0))
        );
        assert_eq!(
            matcher.matches(langid!("zh-TW"), &accepts),
            Some((&langid!("zh-Hant"), 0))
        );
    }
}
