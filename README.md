# language-matcher

There's no language matcher in [`icu4x`](https://github.com/unicode-org/icu4x).
And, if you have noticed, the language matching data in the JSON data of CLDR is broken.

This is a [language matcher](https://www.unicode.org/reports/tr35/tr35.html#EnhancedLanguageMatching) based on the XML data of CLDR.
The distance value is multiplied by 10 to show the difference by paradigm locales.

``` rust
use icu_locid::langid;
use language_matcher::LanguageMatcher;

let matcher = LanguageMatcher::new();

assert_eq!(matcher.distance(langid!("zh-CN"), langid!("zh-Hans")), 0);
assert_eq!(matcher.distance(langid!("zh-HK"), langid!("zh-MO")), 40);

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
```
