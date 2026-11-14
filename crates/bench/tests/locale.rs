//! Which catalog the program speaks: the language of the locale that the environment names, as
//! POSIX orders its variables, and English for a locale without a catalog.

use bench::locale;

/// Environment variables, as name and value.
type Variables = &'static [(&'static str, &'static str)];

#[test]
fn the_locale_is_the_first_of_lc_all_lc_messages_and_lang_that_is_set() {
    let environment = |pairs: Variables| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    };
    let cases: [(Variables, Option<&str>); 5] = [
        (&[], None),
        (&[("LANG", "en_US.UTF-8")], Some("en_US.UTF-8")),
        (
            &[("LANG", "en_US.UTF-8"), ("LC_MESSAGES", "de_DE.UTF-8")],
            Some("de_DE.UTF-8"),
        ),
        (
            &[
                ("LANG", "en_US.UTF-8"),
                ("LC_MESSAGES", "de_DE"),
                ("LC_ALL", "C"),
            ],
            Some("C"),
        ),
        (&[("LC_ALL", ""), ("LANG", "fr_FR")], Some("fr_FR")),
    ];
    for (pairs, expected) in cases {
        assert_eq!(
            locale::locale(environment(pairs)).as_deref(),
            expected,
            "{pairs:?}"
        );
    }
}

#[test]
fn every_locale_without_a_catalog_of_its_own_gets_english() {
    for locale in [
        "en_US.UTF-8",
        "en",
        "C",
        "POSIX",
        "",
        "zh_CN.UTF-8",
        "de_DE@euro",
    ] {
        assert_eq!(locale::catalog(locale).0, "en", "{locale}");
    }
}

#[test]
fn english_counts_in_the_singular_for_one() {
    let words = locale::catalog("en").1;
    assert_eq!(words.item_count(1), "1 test item");
    assert_eq!(words.item_count(8), "8 test items");
}
