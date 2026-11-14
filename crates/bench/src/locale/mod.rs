//! Every word that the program shows: its report, its progress log, its errors and the help of
//! its command line. Code asks [`words`] for each text instead of writing it, so that a language
//! is one catalog: a module that implements the traits below, listed in `LOCALES`. The compiler
//! names every text that a new catalog still lacks. The tests expect the English catalog.

mod download;
mod en;
mod log;
mod program;

use std::sync::OnceLock;

pub use download::DownloadWords;
pub use download::Retry;
pub use log::LogWords;
pub use program::ProgramWords;

/// The texts of one language: every trait of the catalog.
pub trait Words: ProgramWords + LogWords + DownloadWords + Sync {}

impl<T: ProgramWords + LogWords + DownloadWords + Sync> Words for T {}

/// The catalogs, by the language code that a locale such as `en_US.UTF-8` starts with. The first
/// is the one for every other locale.
const LOCALES: &[(&str, &dyn Words)] = &[("en", &en::English)];

/// The catalog of the language that the environment names, chosen at the first call.
pub fn words() -> &'static dyn Words {
    static ACTIVE: OnceLock<&'static dyn Words> = OnceLock::new();
    *ACTIVE.get_or_init(|| {
        let locale = locale(|name| std::env::var(name).ok()).unwrap_or_default();
        catalog(&locale).1
    })
}

/// The locale that the environment names: the first of `LC_ALL`, `LC_MESSAGES` and `LANG` that
/// `lookup` finds set and not empty, as POSIX orders them.
pub fn locale(lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| lookup(name).filter(|value| !value.is_empty()))
}

/// The catalog for a locale such as `en_US.UTF-8`, with its language code: the catalog of the
/// locale's language, or the first one when there is none for it, as for `C` and `POSIX`.
#[must_use]
pub fn catalog(locale: &str) -> (&'static str, &'static dyn Words) {
    let language = locale.split(['_', '.', '@']).next().unwrap_or_default();
    let chosen = LOCALES.iter().find(|(code, _)| *code == language);
    *chosen.unwrap_or(&LOCALES[0])
}
