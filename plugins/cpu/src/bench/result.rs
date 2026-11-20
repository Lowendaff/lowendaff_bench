//! Unified output and per-suite outcome types for `cpu.bench`.

use bench_api::Measurement;
use bench_api::ToolVersion;
use serde::Serialize;

use crate::geekbench;
use crate::hpl;
use crate::passmark;
use crate::prime95;
use crate::seven_zip;
use crate::sysbench;
use crate::unixbench;
use crate::ycruncher;

/// Canonical sub-suite identifiers in execution order.
pub const SUITE_CODES: [&str; 8] = [
    "sysbench",
    "7zip",
    "unixbench",
    "hpl",
    "passmark",
    "ycruncher",
    "prime95",
    "geekbench",
];

/// Default enabled sub-suites (`sysbench`, `7zip`, `unixbench`, `hpl`, `passmark`, `geekbench`;
/// `ycruncher` and `prime95` are opt-in).
pub const DEFAULT_SUITES: [bool; 8] = [true, true, true, true, true, false, false, true];

/// Why an individual sub-suite inside `cpu.bench` was skipped on this host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SuiteSkip {
    /// Upstream suite supports `x86_64` only; records the host's architecture.
    ArchUnsupported(String),
    /// Host lacks the minimum `glibc` required by this sub-suite.
    Glibc {
        /// Minimum `(major, minor)` glibc needed by the sub-suite.
        needed: (u32, u32),
        /// `(major, minor)` glibc found on the host, or `None` on musl / non-glibc hosts.
        found: Option<(u32, u32)>,
    },
    /// Sub-suite requires an IPv4 connection (Geekbench upload / CDN).
    Ipv4Unavailable,
    /// Required tool failed to provision, with the provisioner's reason.
    ToolUnavailable {
        /// Tool name.
        tool: &'static str,
        /// Provisioning failure reason.
        why: String,
    },
}

/// Outcome of one selected sub-suite inside `cpu.bench`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SuiteOutcome<O, F> {
    /// Sub-suite completed cleanly.
    Done {
        /// Measured result.
        output: O,
    },
    /// Sub-suite did not apply or lacked a precondition on this host.
    Skipped {
        /// Skip reason.
        reason: SuiteSkip,
    },
    /// Sub-suite failed (carrying any partial measurement collected before failure).
    Failed {
        /// Partial measurement, if any (such as single-thread score before multi-thread failed).
        partial: Option<O>,
        /// Suite-specific failure reason.
        failure: F,
    },
}

impl<O: Measurement, F> SuiteOutcome<O, F> {
    /// Whether this sub-suite completed cleanly.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        matches!(self, Self::Done { .. })
    }

    /// Whether this sub-suite failed.
    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Reported tool versions from completed or partial output.
    #[must_use]
    pub fn tools(&self) -> Vec<ToolVersion> {
        match self {
            Self::Done { output } => output.tools(),
            Self::Failed {
                partial: Some(output),
                ..
            } => output.tools(),
            Self::Skipped { .. } | Self::Failed { partial: None, .. } => Vec::new(),
        }
    }
}

/// Unified `cpu.bench` output across all selected sub-suites.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Output {
    /// Selected suite codes in execution order when non-default (`empty` when default).
    pub suites: Vec<String>,
    /// `sysbench` CPU prime-sieve outcome, when selected.
    pub sysbench: Option<SuiteOutcome<sysbench::Output, sysbench::Failure>>,
    /// `7zip` LZMA2 MIPS outcome, when selected.
    pub seven_zip: Option<SuiteOutcome<seven_zip::Output, seven_zip::Failure>>,
    /// `unixbench` 12-item System Index outcome, when selected.
    pub unixbench: Option<SuiteOutcome<unixbench::Output, unixbench::Failure>>,
    /// `hpl` dense LU GFLOPS outcome, when selected.
    pub hpl: Option<SuiteOutcome<hpl::Output, hpl::Failure>>,
    /// `passmark` CPU Mark outcome, when selected.
    pub passmark: Option<SuiteOutcome<passmark::Output, passmark::Failure>>,
    /// `ycruncher` Pi computation outcome, when selected.
    pub ycruncher: Option<SuiteOutcome<ycruncher::Output, ycruncher::Failure>>,
    /// `prime95` Small-FFT torture stability outcome, when selected.
    pub prime95: Option<SuiteOutcome<prime95::Output, prime95::Failure>>,
    /// `geekbench` Geekbench 7 outcome, when selected.
    pub geekbench: Option<SuiteOutcome<geekbench::Output, geekbench::Failure>>,
}

impl Output {
    /// Number of selected sub-suites that completed cleanly (`Done`).
    #[must_use]
    pub fn completed_count(&self) -> usize {
        usize::from(self.sysbench.as_ref().is_some_and(SuiteOutcome::is_done))
            + usize::from(self.seven_zip.as_ref().is_some_and(SuiteOutcome::is_done))
            + usize::from(self.unixbench.as_ref().is_some_and(SuiteOutcome::is_done))
            + usize::from(self.hpl.as_ref().is_some_and(SuiteOutcome::is_done))
            + usize::from(self.passmark.as_ref().is_some_and(SuiteOutcome::is_done))
            + usize::from(self.ycruncher.as_ref().is_some_and(SuiteOutcome::is_done))
            + usize::from(self.prime95.as_ref().is_some_and(SuiteOutcome::is_done))
            + usize::from(self.geekbench.as_ref().is_some_and(SuiteOutcome::is_done))
    }

    /// Whether any selected sub-suite failed (`Failed`).
    #[must_use]
    pub fn any_failed(&self) -> bool {
        self.sysbench.as_ref().is_some_and(SuiteOutcome::is_failed)
            || self.seven_zip.as_ref().is_some_and(SuiteOutcome::is_failed)
            || self.unixbench.as_ref().is_some_and(SuiteOutcome::is_failed)
            || self.hpl.as_ref().is_some_and(SuiteOutcome::is_failed)
            || self.passmark.as_ref().is_some_and(SuiteOutcome::is_failed)
            || self.ycruncher.as_ref().is_some_and(SuiteOutcome::is_failed)
            || self.prime95.as_ref().is_some_and(SuiteOutcome::is_failed)
            || self.geekbench.as_ref().is_some_and(SuiteOutcome::is_failed)
    }
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        let mut all = Vec::new();
        if let Some(s) = &self.sysbench {
            all.extend(s.tools());
        }
        if let Some(s) = &self.seven_zip {
            all.extend(s.tools());
        }
        if let Some(s) = &self.unixbench {
            all.extend(s.tools());
        }
        if let Some(s) = &self.hpl {
            all.extend(s.tools());
        }
        if let Some(s) = &self.passmark {
            all.extend(s.tools());
        }
        if let Some(s) = &self.ycruncher {
            all.extend(s.tools());
        }
        if let Some(s) = &self.prime95 {
            all.extend(s.tools());
        }
        if let Some(s) = &self.geekbench {
            all.extend(s.tools());
        }
        all
    }
}

/// Top-level stop reason when zero selected sub-suites in `cpu.bench` completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Failure {
    /// No sub-suite was enabled.
    NoSuitesSelected,
    /// Every selected sub-suite was skipped on this host.
    AllSkipped,
    /// Every runnable sub-suite failed (individual failures are stored in [`Output`]).
    AllFailed,
    /// Interrupted via cancellation token.
    Cancelled,
}
