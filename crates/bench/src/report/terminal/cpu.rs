//! Unified `[CPU performance]` chapter (`cpu.bench`): renders each selected sub-suite (`sysbench`,
//! `7-Zip`, `UnixBench`, `HPL`, `PassMark`, `y-cruncher`, `Prime95`, and `Geekbench 7`) in
//! terminal text and GitHub-Flavored Markdown, plus per-suite skip/failure reasons.

use bench_api::Net;
use bench_host::glibc::GlibcVersion;
use bench_tool::job::Adjustment;
use plugin_cpu::bench;
use plugin_cpu::bench::SuiteOutcome;
use plugin_cpu::bench::SuiteSkip;
use plugin_cpu::geekbench;
use plugin_cpu::geekbench::memory::GIB;
use plugin_cpu::geekbench::result::Memory;
use plugin_cpu::geekbench::result::NoSwap;
use plugin_cpu::hpl;
use plugin_cpu::passmark;
use plugin_cpu::prime95;
use plugin_cpu::seven_zip;
use plugin_cpu::sysbench;
use plugin_cpu::sysbench::measure::MAX_PRIME;
use plugin_cpu::sysbench::measure::SECONDS;
use plugin_cpu::unixbench;
use plugin_cpu::ycruncher;

use super::Out;
use super::tool;
use crate::locale;
use crate::locale::NoSwapWhy;
use crate::locale::Unit;
use crate::orchestrator::Unmet;
use crate::results::Report;

/// Renders the unified `cpu.bench` chapter.
pub(super) fn bench(out: &mut Out, report: &Report<bench::Output, bench::Failure>, indent: &str) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    if !output.suites.is_empty() {
        out.line(&format!(
            "{indent}{}",
            words.cpu_selected_suites(&output.suites)
        ));
    }
    let present = count_present(output);
    let mut first = true;
    let mut spacing = |out: &mut Out, code: &str| {
        if !first && !out.is_markdown() {
            out.line("");
        }
        first = false;
        if present > 1 {
            out.heading(indent, words.cpu_suite_title(code));
        }
    };
    let sub_indent = if present > 1 && !out.is_markdown() {
        format!("{indent}  ")
    } else {
        indent.to_owned()
    };

    if let Some(suite) = &output.sysbench {
        spacing(out, "sysbench");
        render_suite(
            out,
            &sub_indent,
            "sysbench",
            suite,
            render_sysbench,
            sysbench_failure,
        );
    }
    if let Some(suite) = &output.seven_zip {
        spacing(out, "7zip");
        render_suite(
            out,
            &sub_indent,
            "7zip",
            suite,
            render_seven_zip,
            seven_zip_failure,
        );
    }
    if let Some(suite) = &output.unixbench {
        spacing(out, "unixbench");
        render_suite(
            out,
            &sub_indent,
            "unixbench",
            suite,
            render_unixbench,
            unixbench_failure,
        );
    }
    if let Some(suite) = &output.hpl {
        spacing(out, "hpl");
        render_suite(out, &sub_indent, "hpl", suite, render_hpl, hpl_failure);
    }
    if let Some(suite) = &output.passmark {
        spacing(out, "passmark");
        render_suite(
            out,
            &sub_indent,
            "passmark",
            suite,
            render_passmark,
            passmark_failure,
        );
    }
    if let Some(suite) = &output.ycruncher {
        spacing(out, "ycruncher");
        render_suite(
            out,
            &sub_indent,
            "ycruncher",
            suite,
            render_ycruncher,
            ycruncher_failure,
        );
    }
    if let Some(suite) = &output.prime95 {
        spacing(out, "prime95");
        render_suite(
            out,
            &sub_indent,
            "prime95",
            suite,
            render_prime95,
            prime95_failure,
        );
    }
    if let Some(suite) = &output.geekbench {
        spacing(out, "geekbench");
        render_suite(
            out,
            &sub_indent,
            "geekbench",
            suite,
            render_geekbench,
            geekbench_failure,
        );
    }
}

fn count_present(output: &bench::Output) -> usize {
    usize::from(output.sysbench.is_some())
        + usize::from(output.seven_zip.is_some())
        + usize::from(output.unixbench.is_some())
        + usize::from(output.hpl.is_some())
        + usize::from(output.passmark.is_some())
        + usize::from(output.ycruncher.is_some())
        + usize::from(output.prime95.is_some())
        + usize::from(output.geekbench.is_some())
}

fn render_suite<O, F>(
    out: &mut Out,
    indent: &str,
    code: &str,
    suite: &SuiteOutcome<O, F>,
    draw_output: fn(&mut Out, &O, &str),
    format_failure: fn(&F) -> String,
) {
    let words = locale::words();
    match suite {
        SuiteOutcome::Done { output } => draw_output(out, output, indent),
        SuiteOutcome::Skipped { reason } => {
            out.field(
                indent,
                &words.note(),
                &words.cpu_suite_skipped(code, &skip_text(reason)),
            );
        }
        SuiteOutcome::Failed { partial, failure } => {
            if let Some(partial) = partial {
                draw_output(out, partial, indent);
            }
            out.field(
                indent,
                &words.note(),
                &words.cpu_suite_failed(code, &format_failure(failure)),
            );
        }
    }
}

/// Top-level stop reason for `cpu.bench` when zero sub-suites completed. When only one sub-suite
/// ran and failed or skipped, surfaces its exact reason directly.
pub(super) fn bench_failure(output: Option<&bench::Output>, failure: &bench::Failure) -> String {
    let words = locale::words();
    if let Some(out) = output
        && count_present(out) == 1
        && let Some(single_reason) = single_suite_reason(out)
    {
        return single_reason;
    }
    match failure {
        bench::Failure::NoSuitesSelected => words.cpu_no_suites_selected(),
        bench::Failure::AllSkipped => words.cpu_all_skipped(),
        bench::Failure::AllFailed => words.cpu_all_failed(),
        bench::Failure::Cancelled => words.interrupted(),
    }
}

fn single_suite_reason(out: &bench::Output) -> Option<String> {
    if let Some(s) = &out.sysbench {
        return outcome_reason(s, sysbench_failure);
    }
    if let Some(s) = &out.seven_zip {
        return outcome_reason(s, seven_zip_failure);
    }
    if let Some(s) = &out.unixbench {
        return outcome_reason(s, unixbench_failure);
    }
    if let Some(s) = &out.hpl {
        return outcome_reason(s, hpl_failure);
    }
    if let Some(s) = &out.passmark {
        return outcome_reason(s, passmark_failure);
    }
    if let Some(s) = &out.ycruncher {
        return outcome_reason(s, ycruncher_failure);
    }
    if let Some(s) = &out.prime95 {
        return outcome_reason(s, prime95_failure);
    }
    if let Some(s) = &out.geekbench {
        return outcome_reason(s, geekbench_failure);
    }
    None
}

fn outcome_reason<O, F>(
    outcome: &SuiteOutcome<O, F>,
    format_failure: fn(&F) -> String,
) -> Option<String> {
    match outcome {
        SuiteOutcome::Done { .. } => None,
        SuiteOutcome::Skipped { reason } => Some(skip_text(reason)),
        SuiteOutcome::Failed { failure, .. } => Some(format_failure(failure)),
    }
}

fn skip_text(skip: &SuiteSkip) -> String {
    let words = locale::words();
    match skip {
        SuiteSkip::ArchUnsupported(arch) => words.cpu_skip_arch(arch),
        SuiteSkip::Glibc { needed, found } => words.unmet(&Unmet::Glibc {
            needed: GlibcVersion {
                major: needed.0,
                minor: needed.1,
            },
            found: found.map(|(major, minor)| GlibcVersion { major, minor }),
        }),
        SuiteSkip::Ipv4Unavailable => words.unmet(&Unmet::Network(Net::Ipv4)),
        SuiteSkip::ToolUnavailable { tool, why } => words.unmet(&Unmet::Tool {
            name: tool,
            why: why.clone(),
        }),
    }
}

// ---------------------------------------------------------------------------
// 1. sysbench
// ---------------------------------------------------------------------------

fn render_sysbench(out: &mut Out, output: &sysbench::Output, indent: &str) {
    let words = locale::words();
    let parameters = words.sysbench_parameters(
        output.version.as_deref(),
        u64::from(MAX_PRIME),
        u64::from(SECONDS),
    );
    out.line(&format!("{indent}{parameters}"));
    let headers = words.cpu_headers().map(str::to_owned);
    let row = |name: &str, run: &sysbench::result::Run| {
        vec![
            name.to_owned(),
            run.threads.to_string(),
            run.rate.text.clone(),
        ]
    };
    let mut rows = vec![row(words.cpu_row(false), &output.single)];
    let mut runs = vec![(words.cpu_test(false), &output.single)];
    if let Some(multi) = &output.multi {
        rows.push(row(words.cpu_row(true), multi));
        runs.push((words.cpu_test(true), multi));
    }
    out.table(indent, &headers, &rows);
    if let Some(multi) = &output.multi {
        let ratio = multi.rate.value / output.single.rate.value;
        out.field(indent, words.ratio_label(), &words.ratio(ratio));
    }
    for (name, run) in runs {
        let Some(last) = run.retries.last() else {
            continue;
        };
        let note = words.retried(name, run.retries.len(), &sysbench_fault(last));
        out.field(indent, &words.note(), &note);
    }
}

fn sysbench_failure(failure: &sysbench::Failure) -> String {
    let words = locale::words();
    match failure {
        sysbench::Failure::Single(faults) => attempts(faults),
        sysbench::Failure::Multi(faults) => {
            let last = faults.last().map(sysbench_fault).unwrap_or_default();
            words.every_attempt_failed(words.cpu_test(true), &last, faults.len())
        }
        sysbench::Failure::Cancelled => words.interrupted(),
    }
}

fn attempts(faults: &[sysbench::Fault]) -> String {
    let last = faults.last().map(sysbench_fault).unwrap_or_default();
    locale::words().attempts(&last, faults.len())
}

fn sysbench_fault(fault: &sysbench::Fault) -> String {
    match fault {
        sysbench::Fault::Tool(fault) => tool::fault("sysbench", fault, Unit::Seconds),
        sysbench::Fault::NoRate => locale::words().no_rate(),
    }
}

// ---------------------------------------------------------------------------
// 2. 7-Zip
// ---------------------------------------------------------------------------

fn render_seven_zip(out: &mut Out, output: &seven_zip::Output, indent: &str) {
    let words = locale::words();
    out.line(&format!(
        "{indent}{}",
        words.seven_zip_parameters(output.version.as_deref())
    ));
    let headers = words.seven_zip_headers().map(str::to_owned);
    let row = |name: &str, run: &seven_zip::Run| {
        vec![
            name.to_owned(),
            run.threads.to_string(),
            run.compress_mips.to_string(),
            run.decompress_mips.to_string(),
            run.total_mips.to_string(),
        ]
    };
    let mut rows = vec![row(words.cpu_row(false), &output.single)];
    let mut runs = vec![(words.cpu_test(false), &output.single)];
    if let Some(multi) = &output.multi {
        rows.push(row(words.cpu_row(true), multi));
        runs.push((words.cpu_test(true), multi));
    }
    out.table(indent, &headers, &rows);
    if let Some(multi) = &output.multi
        && output.single.total_mips > 0
    {
        let ratio = (multi.total_mips as f64) / (output.single.total_mips as f64);
        out.field(indent, words.ratio_label(), &words.ratio(ratio));
    }
    for (name, run) in runs {
        if let Some(last) = run.retries.last() {
            let note = words.retried(name, run.retries.len(), &seven_zip_fault(last));
            out.field(indent, &words.note(), &note);
        }
    }
}

fn seven_zip_failure(failure: &seven_zip::Failure) -> String {
    let words = locale::words();
    match failure {
        seven_zip::Failure::Single(faults) => {
            let last = faults.last().map(seven_zip_fault).unwrap_or_default();
            words.attempts(&last, faults.len())
        }
        seven_zip::Failure::Multi(faults) => {
            let last = faults.last().map(seven_zip_fault).unwrap_or_default();
            words.every_attempt_failed(words.cpu_test(true), &last, faults.len())
        }
        seven_zip::Failure::Cancelled => words.interrupted(),
    }
}

fn seven_zip_fault(fault: &seven_zip::Fault) -> String {
    match fault {
        seven_zip::Fault::Tool(fault) => tool::fault("7-Zip", fault, Unit::Seconds),
        seven_zip::Fault::NoScores => locale::words().seven_zip_no_scores(),
    }
}

// ---------------------------------------------------------------------------
// 3. UnixBench
// ---------------------------------------------------------------------------

fn render_unixbench(out: &mut Out, output: &unixbench::Output, indent: &str) {
    let words = locale::words();
    out.line(&format!(
        "{indent}{}",
        words.unixbench_parameters(&output.version)
    ));
    out.field(
        indent,
        &words.unixbench_index_label(output.single.copies),
        &format!("{:.1}", output.single.index),
    );
    if let Some(multi) = &output.multi {
        out.field(
            indent,
            &words.unixbench_index_label(multi.copies),
            &format!("{:.1}", multi.index),
        );
        if output.single.index > 0.0 {
            let ratio = multi.index / output.single.index;
            out.field(indent, words.ratio_label(), &words.ratio(ratio));
        }
    }
    let headers = words.unixbench_headers(output.multi.as_ref().map(|m| m.copies));
    let mut rows = Vec::with_capacity(output.single.items.len());
    for single_item in &output.single.items {
        let name = words.unixbench_item_name(single_item.id).to_owned();
        if let Some(multi) = &output.multi {
            let multi_item = multi.items.iter().find(|item| item.id == single_item.id);
            rows.push(vec![
                name,
                format!("{:.1}", single_item.rate),
                multi_item.map_or_else(|| "-".to_owned(), |m| format!("{:.1}", m.rate)),
                single_item.unit.to_owned(),
                format!("{:.1}", single_item.index),
                multi_item.map_or_else(|| "-".to_owned(), |m| format!("{:.1}", m.index)),
            ]);
        } else {
            rows.push(vec![
                name,
                format!("{:.1}", single_item.rate),
                single_item.unit.to_owned(),
                format!("{:.1}", single_item.index),
            ]);
        }
    }
    out.table(indent, &headers, &rows);
}

fn unixbench_failure(failure: &unixbench::Failure) -> String {
    let words = locale::words();
    match failure {
        unixbench::Failure::Single(fault) | unixbench::Failure::Multi(fault) => {
            unixbench_fault(fault)
        }
        unixbench::Failure::Cancelled => words.interrupted(),
    }
}

fn unixbench_fault(fault: &unixbench::Fault) -> String {
    let words = locale::words();
    match fault {
        unixbench::Fault::Tool { item, fault } => {
            tool::fault(&format!("unixbench ({item})"), fault, Unit::Seconds)
        }
        unixbench::Fault::Setup(reason) => words.cpu_setup_failed(reason),
        unixbench::Fault::NoCount { item } => words.unixbench_no_count(item),
    }
}

// ---------------------------------------------------------------------------
// 4. HPL (Linpack)
// ---------------------------------------------------------------------------

fn render_hpl(out: &mut Out, output: &hpl::Output, indent: &str) {
    let words = locale::words();
    out.line(&format!(
        "{indent}{}",
        words.hpl_parameters(
            output.version.as_deref(),
            output.n,
            output.nb,
            output.threads,
        )
    ));
    let [throughput, solve_time, residual] = words.hpl_labels();
    out.field(indent, throughput, &words.hpl_gflops(output.gflops));
    out.field(indent, solve_time, &words.hpl_seconds(output.seconds));
    out.field(
        indent,
        residual,
        &words.hpl_residual_passed(output.residual),
    );
}

fn hpl_failure(failure: &hpl::Failure) -> String {
    let words = locale::words();
    match failure {
        hpl::Failure::Run(hpl::Fault::Tool(fault)) => tool::fault("xhpl", fault, Unit::Minutes),
        hpl::Failure::Run(hpl::Fault::Setup(reason)) => words.cpu_setup_failed(reason),
        hpl::Failure::Run(hpl::Fault::ResidualFailed) => words.hpl_residual_failed(),
        hpl::Failure::Run(hpl::Fault::NoResult) => words.no_result("xhpl"),
        hpl::Failure::Cancelled => words.interrupted(),
    }
}

// ---------------------------------------------------------------------------
// 5. PassMark
// ---------------------------------------------------------------------------

fn render_passmark(out: &mut Out, output: &passmark::Output, indent: &str) {
    let words = locale::words();
    out.line(&format!(
        "{indent}{}",
        words.passmark_parameters(output.version.as_deref())
    ));
    let [cpu_mark, single_thread] = words.passmark_summary_labels();
    out.field(indent, cpu_mark, &format!("{:.1}", output.cpu_mark));
    out.field(
        indent,
        single_thread,
        &format!("{:.1}", output.single_thread),
    );

    let headers = words.passmark_headers().map(str::to_owned);
    let names = words.passmark_subtests();
    let values = [
        output.integer_math,
        output.floating_math,
        output.prime,
        output.sorting,
        output.encryption,
        output.compression,
        output.physics,
        output.extended_instructions,
    ];
    let rows: Vec<Vec<String>> = names
        .iter()
        .zip(values.iter())
        .map(|((name, unit), val)| {
            vec![(*name).to_owned(), format!("{val:.1}"), (*unit).to_owned()]
        })
        .collect();
    out.table(indent, &headers, &rows);
}

fn passmark_failure(failure: &passmark::Failure) -> String {
    let words = locale::words();
    match failure {
        passmark::Failure::Run(passmark::Fault::Tool(fault)) => {
            tool::fault("PassMark", fault, Unit::Minutes)
        }
        passmark::Failure::Run(passmark::Fault::MissingHostLib(lib)) => words.cpu_missing_lib(lib),
        passmark::Failure::Run(passmark::Fault::Io(reason)) => words.cpu_setup_failed(reason),
        passmark::Failure::Run(passmark::Fault::NoScores) => words.passmark_no_scores(),
        passmark::Failure::Cancelled => words.interrupted(),
    }
}

// ---------------------------------------------------------------------------
// 6. y-cruncher
// ---------------------------------------------------------------------------

fn render_ycruncher(out: &mut Out, output: &ycruncher::Output, indent: &str) {
    let words = locale::words();
    out.line(&format!(
        "{indent}{}",
        words.ycruncher_parameters(
            output.version.as_deref(),
            output.digits,
            output.tuning.as_deref(),
        )
    ));
    let [comp, wall, util, eff] = words.ycruncher_labels();
    out.field(indent, comp, &format!("{:.3} s", output.computation_secs));
    out.field(indent, wall, &format!("{:.3} s", output.wall_secs));
    if let Some(u) = output.cpu_utilization_pct {
        out.field(indent, util, &format!("{u:.1}%"));
    }
    if let Some(e) = output.multicore_efficiency_pct {
        out.field(indent, eff, &format!("{e:.1}%"));
    }
}

fn ycruncher_failure(failure: &ycruncher::Failure) -> String {
    let words = locale::words();
    match failure {
        ycruncher::Failure::Run(ycruncher::Fault::Tool(fault)) => {
            tool::fault("y-cruncher", fault, Unit::Minutes)
        }
        ycruncher::Failure::Run(ycruncher::Fault::Setup(reason)) => words.cpu_setup_failed(reason),
        ycruncher::Failure::Run(ycruncher::Fault::ValidationFailed) => {
            words.ycruncher_validation_failed()
        }
        ycruncher::Failure::Run(ycruncher::Fault::NoResult) => words.no_result("y-cruncher"),
        ycruncher::Failure::Cancelled => words.interrupted(),
    }
}

// ---------------------------------------------------------------------------
// 7. Prime95
// ---------------------------------------------------------------------------

fn render_prime95(out: &mut Out, output: &prime95::Output, indent: &str) {
    let words = locale::words();
    out.line(&format!(
        "{indent}{}",
        words.prime95_parameters(
            output.version.as_deref(),
            output.workers,
            output.duration_secs,
            output.fft_desc.as_deref(),
        )
    ));
    let [stability, completed] = words.prime95_labels();
    out.field(indent, stability, &words.prime95_passed(output.warnings));
    out.field(indent, completed, &output.tests_completed.to_string());
}

fn prime95_failure(failure: &prime95::Failure) -> String {
    let words = locale::words();
    match failure {
        prime95::Failure::Run(prime95::Fault::Tool(fault)) => {
            tool::fault("Prime95", fault, Unit::Seconds)
        }
        prime95::Failure::Run(prime95::Fault::MissingHostLib(lib)) => words.cpu_missing_lib(lib),
        prime95::Failure::Run(prime95::Fault::Setup(reason)) => words.cpu_setup_failed(reason),
        prime95::Failure::Run(prime95::Fault::HardwareError(detail)) => {
            words.prime95_hardware_error(detail)
        }
        prime95::Failure::Run(prime95::Fault::NoSummary) => words.prime95_no_summary(),
        prime95::Failure::Cancelled => words.interrupted(),
    }
}

// ---------------------------------------------------------------------------
// 8. Geekbench 7
// ---------------------------------------------------------------------------

fn render_geekbench(out: &mut Out, output: &geekbench::Output, indent: &str) {
    let words = locale::words();
    if let Some(scores) = &output.scores {
        if let Some(version) = &scores.version {
            out.line(&format!("{indent}Geekbench {version}"));
        }
        let [single, multi, link] = words.geekbench_labels();
        out.field(indent, single, &scores.single.to_string());
        out.field(indent, multi, &scores.multi.to_string());
        let url = scores.url.clone().unwrap_or_else(|| words.unavailable());
        out.field(indent, link, &url);
    }
    let note = words.note();
    if let Some(memory) = &output.memory {
        out.field(indent, &note, &memory_note(memory));
    }
    if let Some(first_to_go) = output.first_to_go.as_ref().and_then(first_to_go_note) {
        out.field(indent, &note, &first_to_go);
    }
    if output.scores.is_some()
        && let Some(last) = output.uploads.last()
    {
        let uploads = words.uploads_retried(output.uploads.len(), last);
        out.field(indent, &note, &uploads);
    }
    if let Some(left) = &output.swap_left {
        out.field(indent, &note, &words.swap_left(&left.error, &left.path));
    }
}

fn geekbench_failure(failure: &geekbench::Failure) -> String {
    let words = locale::words();
    match failure {
        geekbench::Failure::Upload(reason) => words.upload_failed(reason),
        geekbench::Failure::Run(geekbench::Fault::Tool(fault)) => {
            tool::fault("Geekbench", fault, Unit::Minutes)
        }
        geekbench::Failure::Run(geekbench::Fault::NoScores) => words.no_scores(),
        geekbench::Failure::OutOfMemory => words.out_of_memory(),
        geekbench::Failure::Cancelled => words.interrupted(),
    }
}

fn memory_note(memory: &Memory) -> String {
    let words = locale::words();
    match memory {
        Memory::Swapped {
            need,
            available,
            gib,
        } => words.swapped(need / GIB, &tenths(*available), *gib),
        Memory::Short {
            need,
            available,
            why,
        } => {
            let free = match why {
                NoSwap::NoRoom(Some(free)) => Some(tenths(*free)),
                _ => None,
            };
            let why = match why {
                NoSwap::Unused => NoSwapWhy::Unused,
                NoSwap::NoRoom(_) => NoSwapWhy::NoRoom(free.as_deref()),
                NoSwap::Denied => NoSwapWhy::Denied,
                NoSwap::Unsupported => NoSwapWhy::Unsupported,
                NoSwap::Error(error) => NoSwapWhy::Error(error),
            };
            words.memory_short(need / GIB, &tenths(*available), why)
        }
    }
}

fn first_to_go_note(adjustment: &Adjustment) -> Option<String> {
    let words = locale::words();
    match adjustment {
        Adjustment::Done => None,
        Adjustment::NotSet(error) => Some(words.first_to_go_not_set(error)),
        Adjustment::NotRestored(error) => Some(words.oom_not_restored(error)),
    }
}

fn tenths(bytes: u64) -> String {
    let tenths = u128::from(bytes) * 10 / u128::from(GIB);
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    #[test]
    fn tenths_handles_u64_max_without_overflow() {
        assert_eq!(super::tenths(u64::MAX), "17179869183.9");
    }
}
