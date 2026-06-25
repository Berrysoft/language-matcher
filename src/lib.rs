//! A language matcher with CLDR.
//!
//! The "sync" feature of `icu_provider` is enabled because we like Sync.

#![warn(missing_docs)]
#![deny(unsafe_code)]

use icu_locale::{LanguageIdentifier, LocaleExpander};
use std::collections::{HashMap, HashSet};

include!(concat!(env!("OUT_DIR"), "/language_info.rs"));

type Variables = HashMap<&'static str, HashSet<&'static str>>;

trait Rule<T> {
    fn matches(self, tag: T, vars: &Variables) -> bool;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
enum SubTagRule {
    Str(&'static str),
    Var(&'static str),
    VarExclude(&'static str),
    All,
}

impl Rule<&'_ str> for &'_ SubTagRule {
    fn matches(self, tag: &str, vars: &Variables) -> bool {
        match self {
            SubTagRule::Str(s) => *s == tag,
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

#[derive(Debug, PartialEq)]
struct LanguageIdentifierRule {
    pub language: SubTagRule,
    pub script: Option<SubTagRule>,
    pub region: Option<SubTagRule>,
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

#[derive(Debug, PartialEq)]
struct ParadigmLocales {
    pub locales: &'static [LanguageIdentifier],
}

#[derive(Debug, PartialEq)]
struct MatchVariable {
    pub id: &'static str,
    pub value: &'static [&'static str],
}

#[derive(Debug, PartialEq)]
struct LanguageMatch {
    pub desired: LanguageIdentifierRule,
    pub supported: LanguageIdentifierRule,
    pub distance: u16,
    pub oneway: bool,
}

#[derive(Debug, PartialEq)]
struct LanguageMatches {
    pub paradigm_locales: ParadigmLocales,
    pub match_variable: &'static [MatchVariable],
    pub language_match: &'static [LanguageMatch],
}

#[derive(Debug, PartialEq)]
struct LanguageMatching {
    pub language_matches: LanguageMatches,
}

#[derive(Debug, PartialEq)]
struct SupplementalData {
    pub language_matching: LanguageMatching,
}

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
    paradigm: HashSet<&'static LanguageIdentifier>,
    vars: Variables,
    rules: &'static [LanguageMatch],
    expander: LocaleExpander,
}

impl From<&SupplementalData> for LanguageMatcher {
    fn from(data: &SupplementalData) -> Self {
        let expander = LocaleExpander::new_extended();

        let matches = &data.language_matching.language_matches;

        let paradigm = HashSet::from_iter(matches.paradigm_locales.locales);
        let vars = matches
            .match_variable
            .iter()
            .map(|MatchVariable { id, value }| (*id, HashSet::from_iter(value.iter().copied())))
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
        Self::from(&LANGUAGE_INFO)
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
        for rule in self.rules {
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
