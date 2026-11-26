//! Every word that the program shows: its report, its progress log, its errors, its TUI, and the
//! help of its command line. Code asks [`words`] for each text instead of writing it, so that a
//! language is one catalog: a module that implements the traits below. The compiler names every
//! text that a new catalog still lacks. Unless `--lang` or the TUI selects another language, the
//! active catalog is always English (`Language::En`), regardless of `LC_*` / `LANG` variables.

mod download;
mod en;
mod host;
mod items;
mod ja;
mod log;
mod program;
mod report;
mod tui;
mod zh_cn;
mod zh_tw;

use std::cell::Cell;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering;

pub use download::Download;
pub use download::DownloadWords;
pub use download::Retry;
pub use host::Cores;
pub use host::HostHeader;
pub use host::HostLabel;
pub use host::HostWords;
pub use host::NoEcc;
pub use host::NoModules;
pub use items::BgpField;
pub use items::BgpIssueRef;
pub use items::BgpWords;
pub use items::CloudflareChoice;
pub use items::CloudflareField;
pub use items::CloudflareIssueRef;
pub use items::CloudflareWords;
pub use items::CpuWords;
pub use items::DiskWords;
pub use items::FioRow;
pub use items::InetspeedChoice;
pub use items::InetspeedProbeFaultRef;
pub use items::InetspeedRoundIssueRef;
pub use items::InetspeedWarningRef;
pub use items::InetspeedWords;
pub use items::IpChoice;
pub use items::IpField;
pub use items::IpTable;
pub use items::IpWords;
pub use items::ListRule;
pub use items::LookupProblem;
pub use items::MediaCell;
pub use items::MediaChoice;
pub use items::MediaWords;
pub use items::MemoryWords;
pub use items::NetworkWords;
pub use items::NoSwapWhy;
pub use items::Probe;
pub use items::TaierCell;
pub use items::TaierChoice;
pub use items::TaierListProblem;
pub use items::TaierRule;
pub use items::TaierSelectorProblem;
pub use items::TaierWords;
pub use items::TcpingChoice;
pub use items::TcpingListProblem;
pub use items::TcpingWords;
pub use items::TraceChoice;
pub use items::TraceList;
pub use items::TraceListProblem;
pub use items::TraceRule;
pub use items::TraceWords;
pub use items::Verdict;
pub use items::WayCell;
pub use log::LogWords;
pub use program::ProgramWords;
pub use report::ReportWords;
pub use report::Totals;
pub use report::Unit;
pub use tui::TuiWords;

/// Supported interface and report languages (`--lang` and the interactive TUI selector).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Language {
    /// English (`en`, default).
    #[default]
    En,
    /// Simplified Chinese (`zh-cn`).
    #[value(name = "zh-cn", alias = "zh-hans")]
    ZhCn,
    /// Traditional Chinese (`zh-tw`).
    #[value(name = "zh-tw", alias = "zh-hant", alias = "zh-hk")]
    ZhTw,
    /// Japanese (`ja`).
    Ja,
}

impl Language {
    /// All supported languages in UI display order.
    pub const ALL: [Self; 4] = [Self::En, Self::ZhCn, Self::ZhTw, Self::Ja];

    /// Canonical CLI code (`en`, `zh-cn`, `zh-tw`, `ja`).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhCn => "zh-cn",
            Self::ZhTw => "zh-tw",
            Self::Ja => "ja",
        }
    }

    /// Native language name shown in the TUI header badge and language picker modal.
    #[must_use]
    pub const fn endonym(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::ZhCn => "\u{7b80}\u{4f53}\u{4e2d}\u{6587}",
            Self::ZhTw => "\u{7e41}\u{9ad4}\u{4e2d}\u{6587}",
            Self::Ja => "\u{65e5}\u{672c}\u{8a9e}",
        }
    }

    const fn to_u8(self) -> u8 {
        match self {
            Self::En => 0,
            Self::ZhCn => 1,
            Self::ZhTw => 2,
            Self::Ja => 3,
        }
    }

    const fn from_u8(raw: u8) -> Self {
        match raw {
            1 => Self::ZhCn,
            2 => Self::ZhTw,
            3 => Self::Ja,
            _ => Self::En,
        }
    }
}

/// Parses a CLI `--lang` value (case-insensitively, including `zh-hans`, `zh-hant`, `zh-hk`).
#[must_use]
pub fn parse_language(raw: &str) -> Option<Language> {
    use clap::ValueEnum;

    Language::from_str(raw.trim(), true).ok()
}

/// The texts of one language: every trait of the catalog.
pub trait Words:
    ProgramWords
    + LogWords
    + DownloadWords
    + ReportWords
    + HostWords
    + CpuWords
    + MemoryWords
    + DiskWords
    + NetworkWords
    + TaierWords
    + CloudflareWords
    + InetspeedWords
    + TcpingWords
    + TraceWords
    + BgpWords
    + IpWords
    + MediaWords
    + TuiWords
    + Sync
{
}

impl<T> Words for T where
    T: ProgramWords
        + LogWords
        + DownloadWords
        + ReportWords
        + HostWords
        + CpuWords
        + MemoryWords
        + DiskWords
        + NetworkWords
        + TaierWords
        + CloudflareWords
        + InetspeedWords
        + TcpingWords
        + TraceWords
        + BgpWords
        + IpWords
        + MediaWords
        + TuiWords
        + Sync
{
}

static ACTIVE: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static OVERRIDE: Cell<Option<Language>> = const { Cell::new(None) };
}

struct OverrideGuard(Option<Language>);

impl Drop for OverrideGuard {
    fn drop(&mut self) {
        OVERRIDE.with(|slot| slot.set(self.0));
    }
}

/// Returns the active [`Language`] on the current thread (or the process-wide selection when no
/// [`with_language`] override is active).
#[must_use]
pub fn current_language() -> Language {
    OVERRIDE
        .with(Cell::get)
        .unwrap_or_else(|| Language::from_u8(ACTIVE.load(Ordering::Relaxed)))
}

/// Sets the active [`Language`]. When called inside a [`with_language`] scope (such as parallel
/// tests), updates only the current thread's scoped language; otherwise updates the process-wide
/// active language for all threads.
pub fn set_language(lang: Language) {
    let scoped = OVERRIDE.with(|slot| {
        if slot.get().is_some() {
            slot.set(Some(lang));
            true
        } else {
            false
        }
    });
    if !scoped {
        ACTIVE.store(lang.to_u8(), Ordering::Relaxed);
    }
}

/// Runs `f` with `lang` active on the current thread, restoring the previous thread-local state
/// afterward even if `f` unwinds. Used by JSON report rendering (`Language::En`) and parallel
/// multi-language tests.
pub fn with_language<R>(lang: Language, f: impl FnOnce() -> R) -> R {
    let prev = OVERRIDE.with(|slot| {
        let old = slot.get();
        slot.set(Some(lang));
        old
    });
    let _guard = OverrideGuard(prev);
    f()
}

/// Returns the static [`Words`] catalog for `lang`.
#[must_use]
pub fn catalog_for(lang: Language) -> &'static dyn Words {
    match lang {
        Language::En => &en::English,
        Language::ZhCn => &zh_cn::SimplifiedChinese,
        Language::ZhTw => &zh_tw::TraditionalChinese,
        Language::Ja => &ja::Japanese,
    }
}

/// Returns the [`Words`] catalog of the currently active language.
#[must_use]
pub fn words() -> &'static dyn Words {
    catalog_for(current_language())
}
