//! The network group's chapter: network speed (iperf3), a table for each family, with a note on
//! each direction that took several tries or failed; route tracing (NextTrace), every hop of each
//! target's route; and why either item stopped short, in the catalog's words.

use std::net::IpAddr;

use bench_tool::NEXTTRACE;
use bench_tool::job;
use plugin_network::bgp;
use plugin_network::bgp::BgpPath;
use plugin_network::bgp::FamilyBgp;
use plugin_network::bgp::RpkiStatus;
use plugin_network::bgp::asns;
use plugin_network::cloudflare;
use plugin_network::cloudflare::FamilyCloudflare;
use plugin_network::inetspeed;
use plugin_network::inetspeed::RoundResult;
use plugin_network::inetspeed::TransferDirection;
use plugin_network::iperf3;
use plugin_network::iperf3::result::Tested;
use plugin_network::iperf3::result::Way;
use plugin_network::iperf3::servers;
use plugin_network::iperf3::servers::ListError;
use plugin_network::iperf3::servers::Rule;
use plugin_network::taier;
use plugin_network::taier::NodeOutcome;
use plugin_network::taier::Operator;
use plugin_network::taier::TestedNode;
use plugin_network::trace;
use plugin_network::trace::Choice;
use plugin_network::trace::exchanges;
use plugin_network::trace::lists;
use plugin_network::trace::lookup::LookupError;
use plugin_network::trace::result::ExchangeList;
use plugin_network::trace::result::Route;
use plugin_network::trace::result::Target;
use plugin_network::trace::result::Trace;
use plugin_network::trace::result::TraceError;
use plugin_network::trace::result::Unavailable;
use plugin_network::trace::targets;

use super::super::bgp_graph;
use super::Out;
use super::tool;
use crate::locale;
use crate::locale::BgpField;
use crate::locale::BgpIssueRef;
use crate::locale::CloudflareChoice;
use crate::locale::CloudflareField;
use crate::locale::CloudflareIssueRef;
use crate::locale::InetspeedChoice;
use crate::locale::InetspeedProbeFaultRef;
use crate::locale::InetspeedRoundIssueRef;
use crate::locale::InetspeedWarningRef;
use crate::locale::ListRule;
use crate::locale::LookupProblem;
use crate::locale::TaierCell;
use crate::locale::TaierChoice;
use crate::locale::TaierListProblem;
use crate::locale::TaierRule;
use crate::locale::TraceChoice;
use crate::locale::TraceList;
use crate::locale::TraceListProblem;
use crate::locale::TraceRule;
use crate::locale::Unit;
use crate::locale::WayCell;
use crate::network;
use crate::results::Report;

/// What a missing value shows.
const MISSING: &str = "—";

/// Network speed: the method, a note on what was asked for and is not in the list, then a table
/// for each family with its notes.
pub(super) fn iperf3(
    out: &mut Out,
    report: &Report<iperf3::Output, iperf3::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let settings = output.settings;
    out.line(&format!(
        "{indent}{}",
        words.iperf3_method(
            output.version.as_deref(),
            settings.streams,
            settings.seconds,
            settings.rate
        )
    ));
    if !output.unknown.is_empty() {
        out.field(
            indent,
            &words.note(),
            &words.unknown_servers(&output.unknown),
        );
    }
    for (ipv6, tested) in [(false, &output.ipv4), (true, &output.ipv6)] {
        if tested.is_empty() {
            continue;
        }
        out.heading(indent, words.family_title(ipv6));
        let headers = words.network_headers().map(str::to_owned);
        let rows: Vec<Vec<String>> = tested.iter().map(row).collect();
        out.table(indent, &headers, &rows);
        for server in tested {
            for note in notes(server) {
                out.field(indent, &words.note(), &note);
            }
        }
    }
}

/// A server's row: its place, its provider, its speed each way, and its latency.
fn row(tested: &Tested) -> Vec<String> {
    vec![
        locale::words().place(&tested.location, &tested.country),
        tested.provider.clone(),
        cell(&tested.upload),
        cell(&tested.download),
        tested.latency().map_or_else(|| MISSING.to_owned(), latency),
    ]
}

/// A direction's cell: its speed, or what kept it from one.
fn cell(way: &Way) -> String {
    let words = locale::words();
    let kind = match way {
        Way::Measured(speed) => return bits(speed.bits),
        Way::Busy => WayCell::Busy,
        Way::Unreachable => WayCell::Unreachable,
        Way::Fault(job::Fault::TimedOut(_)) => WayCell::TimedOut,
        Way::Error(_) | Way::Fault(_) | Way::Nothing => WayCell::Error,
        Way::Skipped => WayCell::NotTried,
    };
    words.way_cell(kind).to_owned()
}

/// The notes on a server: each direction that took several tries, or that failed for a reason
/// that its cell does not tell.
fn notes(tested: &Tested) -> Vec<String> {
    let words = locale::words();
    let place = words.place(&tested.location, &tested.country);
    let mut notes = Vec::new();
    for (download, way) in [(false, &tested.upload), (true, &tested.download)] {
        let reason = match way {
            Way::Measured(speed) if speed.tries > 1 => {
                notes.push(words.tries(&place, download, speed.tries));
                continue;
            }
            Way::Error(error) => error.clone(),
            Way::Fault(
                fault @ (job::Fault::Exit { .. }
                | job::Fault::Signal { .. }
                | job::Fault::Spawn(_)
                | job::Fault::Read(_)),
            ) => tool::fault("iperf3", fault, Unit::Seconds),
            Way::Nothing => words.nothing_measured(),
            Way::Measured(_)
            | Way::Busy
            | Way::Unreachable
            | Way::Skipped
            | Way::Fault(job::Fault::TimedOut(_)) => continue,
        };
        notes.push(words.way_failed(&place, download, &reason));
    }
    notes
}

/// Bits per second as iperf3 prints them: from 1 Gbit/s on, in Gbit/s, else in Mbit/s, with
/// three digits or more.
#[must_use]
pub fn bits(bits: u64) -> String {
    let (value, unit) = if bits >= 1_000_000_000 {
        (bits as f64 / 1e9, "Gbit/s")
    } else {
        (bits as f64 / 1e6, "Mbit/s")
    };
    format!("{} {unit}", digits(value))
}

/// A round-trip time in microseconds, in milliseconds with three digits or more.
#[must_use]
pub fn latency(micros: u64) -> String {
    format!("{} ms", digits(micros as f64 / 1000.0))
}

/// A value with three digits or more: none after the point from 100, one from 10, two below.
fn digits(value: f64) -> String {
    if value >= 99.95 {
        format!("{value:.0}")
    } else if value >= 9.995 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

/// Why network speed stopped short.
pub(super) fn iperf3_failure(failure: &iperf3::Failure) -> String {
    let words = locale::words();
    match failure {
        iperf3::Failure::Unreadable(reason) => words.list_unreadable(reason),
        iperf3::Failure::List(error) => list_error(error),
        iperf3::Failure::NoServer(unknown) => words.no_server(unknown),
        iperf3::Failure::NoFamily => words.no_family(),
        iperf3::Failure::NoResult => words.no_speed(),
        iperf3::Failure::Cancelled => words.interrupted(),
    }
}

/// Why a server list cannot be used.
fn list_error(error: &ListError) -> String {
    let words = locale::words();
    match error {
        ListError::TooLarge => words.list_too_large(servers::LIMIT),
        ListError::Format(reason) => words.list_format(reason),
        ListError::Newer(version) => words.list_newer(*version),
        ListError::Version(version) => words.list_version(*version),
        ListError::TooMany(count) => words.list_too_many(*count, servers::MOST_SERVERS),
        ListError::Server(position, rule) => {
            let rule = match rule {
                Rule::Id => ListRule::Id,
                Rule::Country => ListRule::Country,
                Rule::Host => ListRule::Host,
                Rule::Ports => ListRule::Ports,
                Rule::Families => ListRule::Families,
                Rule::Text => ListRule::Text,
            };
            words.list_rule(*position, rule)
        }
        ListError::Duplicate(id) => words.list_duplicate(id),
    }
}

/// China carrier speed (`network.taier`): method line, optional unknown-city note, results table,
/// and per-slot fallback or failure notes.
pub(super) fn taier(out: &mut Out, report: &Report<taier::Output, taier::Failure>, indent: &str) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let settings = &output.settings;
    let non_default_ops = if settings.operators == Operator::DEFAULTS {
        Vec::new()
    } else {
        settings
            .operators
            .iter()
            .map(|op| op.code().to_owned())
            .collect()
    };
    let choice = TaierChoice {
        streams: settings.streams,
        seconds: settings.seconds,
        selections: settings.selections.clone(),
        cities: settings.cities.clone(),
        operators: non_default_ops,
        custom_list: settings.custom_list,
    };
    out.line(&format!("{indent}{}", words.taier_method(&choice)));
    if !output.unknown.is_empty() {
        out.field(
            indent,
            &words.note(),
            &words.taier_unknown_cities(&output.unknown),
        );
    }
    if output.results.is_empty() {
        return;
    }
    let headers = words.taier_headers().map(str::to_owned);
    let rows: Vec<Vec<String>> = output.results.iter().map(taier_row).collect();
    out.table_left(indent, &headers, &rows);
    for node in &output.results {
        let place = words.taier_place(&node.city, &node.province);
        let op = words.taier_operator(node.operator.code());
        match &node.outcome {
            NodeOutcome::Measured(_) if node.tried_nodes > 1 => {
                out.field(
                    indent,
                    &words.note(),
                    &words.taier_fallback(&place, op, node.tried_nodes),
                );
            }
            NodeOutcome::Failed(reason) => {
                out.field(
                    indent,
                    &words.note(),
                    &words.taier_node_failed(&place, op, reason),
                );
            }
            _ => {}
        }
    }
}

fn taier_row(node: &TestedNode) -> Vec<String> {
    let words = locale::words();
    let place = words.taier_place(&node.city, &node.province);
    let operator = words.taier_operator(node.operator.code()).to_owned();
    let (down, up) = match &node.outcome {
        NodeOutcome::Measured(measured) => (bits(measured.download_bps), bits(measured.upload_bps)),
        NodeOutcome::Busy => repeated_taier_cell(words.taier_cell(TaierCell::Busy)),
        NodeOutcome::Unreachable => repeated_taier_cell(words.taier_cell(TaierCell::Unreachable)),
        NodeOutcome::AuthRejected => repeated_taier_cell(words.taier_cell(TaierCell::AuthRejected)),
        NodeOutcome::TimedOut => repeated_taier_cell(words.taier_cell(TaierCell::TimedOut)),
        NodeOutcome::Failed(_) => repeated_taier_cell(words.taier_cell(TaierCell::Error)),
    };
    vec![place, operator, down, up]
}

fn repeated_taier_cell(label: &str) -> (String, String) {
    let text = label.to_owned();
    (text.clone(), text)
}

/// Why `network.taier` stopped short.
pub(super) fn taier_failure(failure: &taier::Failure) -> String {
    let words = locale::words();
    match failure {
        taier::Failure::MissingList => words.taier_list_problem(TaierListProblem::Missing),
        taier::Failure::Unreadable(reason) => {
            words.taier_list_problem(TaierListProblem::Unreadable(reason))
        }
        taier::Failure::List(error) => taier_list_error(error),
        taier::Failure::NoNode(unknown) => words.taier_no_node(unknown),
        taier::Failure::NoResult => words.taier_no_result(),
        taier::Failure::Cancelled => words.interrupted(),
    }
}

fn taier_list_error(error: &taier::nodes::ListError) -> String {
    use taier::nodes::ListError as E;
    use taier::nodes::Rule as R;

    let problem = match error {
        E::TooLarge => TaierListProblem::TooLarge(taier::nodes::LIMIT),
        E::Format(reason) => TaierListProblem::Format(reason),
        E::Newer(version) => TaierListProblem::Newer(*version),
        E::Version(version) => TaierListProblem::Version(*version),
        E::Empty => TaierListProblem::Empty,
        E::TooMany(count) => TaierListProblem::TooMany(*count, taier::nodes::MOST_LOCATIONS),
        E::Location(position, rule) => {
            let rule = match rule {
                R::Id => TaierRule::Id,
                R::Province => TaierRule::Province,
                R::City => TaierRule::City,
                R::Nodes => TaierRule::Nodes,
                R::Operator => TaierRule::Operator,
                R::Ip => TaierRule::Ip,
                R::Port => TaierRule::Port,
                R::DuplicateEndpoint => TaierRule::DuplicateEndpoint,
            };
            TaierListProblem::Rule(*position, rule)
        }
        E::Duplicate(id) => TaierListProblem::Duplicate(id),
    };
    locale::words().taier_list_problem(problem)
}

/// Route tracing: NextTrace's method, the notes on the ids that no target has and on the exchange
/// list, then for each family each target's line and the table of its hops. An address of the
/// host's own, of `own`, is masked unless `show_ip` is set.
pub(super) fn trace(
    out: &mut Out,
    report: &Report<trace::Output, trace::Failure>,
    indent: &str,
    own: &[IpAddr],
    show_ip: bool,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let settings = &output.settings;
    let choice = TraceChoice {
        groups: settings
            .groups
            .iter()
            .map(|group| group_name(*group).to_owned())
            .collect(),
        targets: settings.targets.clone(),
        list: settings.list,
        geo: settings.geo,
    };
    out.line(&format!(
        "{indent}{}",
        words.trace_method(NEXTTRACE.version, &choice)
    ));
    if !output.unknown.is_empty() {
        out.field(
            indent,
            &words.note(),
            &words.unknown_targets(&output.unknown),
        );
    }
    if let Some(reason) = exchanges_missing(&output.exchanges) {
        out.field(indent, &words.note(), &words.no_exchanges(&reason));
    }
    if out.is_markdown() {
        trace_markdown(out, output, indent, own, show_ip);
        return;
    }
    for (ipv6, traced) in [(false, &output.ipv4), (true, &output.ipv6)] {
        if traced.is_empty() {
            continue;
        }
        out.heading(indent, words.family_title(ipv6));
        for target in traced {
            let outcome = route_outcome(&target.route);
            let heading = words.trace_heading(
                &target_name(&target.target),
                target.host.as_deref(),
                &outcome,
            );
            out.line(&format!("{indent}{heading}"));
            if let Route::Traced(trace) = &target.route {
                let headers = words.trace_headers().map(str::to_owned);
                out.table_left(indent, &headers, &hop_rows(trace, own, show_ip));
            }
        }
    }
}

fn trace_markdown(
    out: &mut Out,
    output: &trace::Output,
    indent: &str,
    own: &[IpAddr],
    show_ip: bool,
) {
    let words = locale::words();
    let headers = words.trace_headers().map(str::to_owned);
    for (group, code) in [
        (targets::Group::Services, "services"),
        (targets::Group::China, "china"),
        (targets::Group::International, "international"),
    ] {
        let mut entries = Vec::new();
        for (ipv6, list) in [(false, &output.ipv4), (true, &output.ipv6)] {
            for item in list {
                if item.target.group() == group {
                    entries.push((ipv6, item));
                }
            }
        }
        if entries.is_empty() {
            continue;
        }
        out.subsection(words.trace_group_title(code));
        for (ipv6, target) in entries {
            let outcome = route_outcome(&target.route);
            let title = words.trace_md_target_title(&target_name(&target.target), ipv6);
            out.heading(indent, &title);
            out.line(&words.trace_md_target_line(target.host.as_deref(), &outcome));
            if let Route::Traced(trace) = &target.route {
                out.table_left(indent, &headers, &hop_rows(trace, own, show_ip));
            }
        }
    }
}

fn route_outcome(route: &Route) -> String {
    let words = locale::words();
    match route {
        Route::Traced(trace) if trace.reached => words.reached(trace.hops.len()),
        Route::Traced(trace) => words.not_reached(trace.hops.len()),
        Route::Unavailable(why) => words.unavailable_lookup(&unavailable(why)),
        Route::Failed(error) => trace_error(error),
    }
}

/// A group's name as the configuration writes it.
fn group_name(group: Choice) -> &'static str {
    match group {
        Choice::Services => "services",
        Choice::China => "china",
        Choice::International => "international",
        Choice::All => "all",
    }
}

/// A target's name: a cache server's, or the list's.
fn target_name(target: &Target) -> String {
    let words = locale::words();
    match target {
        Target::YouTube => words.youtube().to_owned(),
        Target::Netflix(place) => words.netflix(
            place
                .as_ref()
                .map(|place| (place.city.as_str(), place.country.as_str())),
        ),
        Target::Listed { name, .. } => name.clone(),
    }
}

/// A trace's rows: each hop's number, then for each address that answered it, the address, its
/// AS number, its network with its exchange, its place and its latency; `*` for a hop that no
/// probe got an answer from. A second address of a hop takes a row of its own, under the same
/// number.
fn hop_rows(trace: &Trace, own: &[IpAddr], show_ip: bool) -> Vec<Vec<String>> {
    let words = locale::words();
    let mut rows = Vec::new();
    let numbered_hops = (1_usize..).zip(&trace.hops);
    for (number, hop) in numbered_hops {
        if hop.answers.is_empty() {
            let mut row = vec![number.to_string(), "*".to_owned()];
            row.resize(6, String::new());
            rows.push(row);
            continue;
        }
        for (index, answer) in hop.answers.iter().enumerate() {
            let hop = if index == 0 {
                number.to_string()
            } else {
                String::new()
            };
            let address = if !show_ip && own.contains(&answer.address) {
                network::mask(answer.address)
            } else {
                answer.address.to_string()
            };
            let asn = answer.asn.clone().unwrap_or_else(|| MISSING.to_owned());
            let network = match &answer.exchange {
                Some(exchange) => words.at_exchange(answer.network.as_deref(), exchange),
                None => answer.network.clone().unwrap_or_else(|| MISSING.to_owned()),
            };
            let place = answer.place.clone().unwrap_or_else(|| MISSING.to_owned());
            rows.push(vec![
                hop,
                address,
                asn,
                network,
                place,
                latency(answer.latency),
            ]);
        }
    }
    rows
}

/// Why a cache server could not be looked up.
fn unavailable(why: &Unavailable) -> String {
    let words = locale::words();
    match why {
        Unavailable::Unfetched(reason) => reason.clone(),
        Unavailable::Unreadable(reason) => words.answer_unreadable(reason),
        Unavailable::Answer(error) => words.lookup_problem(match error {
            LookupError::NoServer => LookupProblem::NoServer,
            LookupError::Id => LookupProblem::Id,
            LookupError::Format => LookupProblem::Format,
            LookupError::NoTarget => LookupProblem::NoTarget,
            LookupError::Url => LookupProblem::Url,
            LookupError::Place => LookupProblem::Place,
        }),
    }
}

/// Why NextTrace gave no route.
fn trace_error(error: &TraceError) -> String {
    match error {
        TraceError::Error(error) => error.clone(),
        TraceError::Nothing => locale::words().traced_nothing(),
        TraceError::Fault(fault) => tool::fault(NEXTTRACE.name, fault, Unit::Seconds),
    }
}

/// Why the run has no exchange names, when it has none.
fn exchanges_missing(list: &ExchangeList) -> Option<String> {
    let words = locale::words();
    match list {
        ExchangeList::Read(_) => None,
        ExchangeList::Missing(reason) => Some(reason.clone()),
        ExchangeList::Unreadable(reason) => Some(
            words.trace_list_problem(TraceList::Exchanges, TraceListProblem::Unreadable(reason)),
        ),
        ExchangeList::Unusable(error) => Some(trace_list_problem(TraceList::Exchanges, error)),
    }
}

/// Why route tracing stopped short.
pub(super) fn trace_failure(failure: &trace::Failure) -> String {
    let words = locale::words();
    match failure {
        trace::Failure::Unreadable(reason) => {
            words.trace_list_problem(TraceList::Targets, TraceListProblem::Unreadable(reason))
        }
        trace::Failure::List(error) => trace_list_problem(TraceList::Targets, error),
        trace::Failure::NoTarget(unknown) => words.no_target(unknown),
        trace::Failure::NoFamily => words.no_target_family(),
        trace::Failure::NoRoute => words.no_target_route(),
        trace::Failure::Cancelled => words.interrupted(),
    }
}

/// Why one of route tracing's lists cannot be used.
fn trace_list_problem(list: TraceList, error: &lists::ListError) -> String {
    let most = match list {
        TraceList::Targets => targets::MOST_TARGETS,
        TraceList::Exchanges => exchanges::MOST_PORTS,
    };
    let problem = match error {
        lists::ListError::TooLarge => TraceListProblem::TooLarge(lists::LIMIT),
        lists::ListError::Format(reason) => TraceListProblem::Format(reason),
        lists::ListError::Newer(version) => TraceListProblem::Newer(*version),
        lists::ListError::Version(version) => TraceListProblem::Version(*version),
        lists::ListError::TooMany(count) => TraceListProblem::TooMany(*count, most),
        lists::ListError::Entry(position, rule) => {
            let rule = match rule {
                lists::Rule::Id => TraceRule::Id,
                lists::Rule::Group => TraceRule::Group,
                lists::Rule::Name => TraceRule::Name,
                lists::Rule::Asn => TraceRule::Asn,
                lists::Rule::Ipv4 => TraceRule::Ipv4,
                lists::Rule::Ipv6 => TraceRule::Ipv6,
                lists::Rule::Address => TraceRule::Address,
                lists::Rule::Exchange => TraceRule::Exchange,
            };
            TraceListProblem::Rule(*position, rule)
        }
        lists::ListError::Duplicate(key) => TraceListProblem::Duplicate(key),
    };
    locale::words().trace_list_problem(list, problem)
}

/// Maximum observed paths listed in the per-family `AS_PATH` table.
const SHOWN_BGP_PATHS: usize = 8;

/// Maximum direct upstreams listed in the summary table cell.
const SHOWN_UPSTREAMS: usize = 4;

/// BGP paths (`network.bgp`): summary table across IPv4 and IPv6, per-family topology link
/// (`imageupload.app` SVG link in `text` mode, `mermaid` `flowchart LR` in `markdown` mode), and
/// top observed `AS_PATH` rows.
pub(super) fn bgp(
    out: &mut Out,
    report: &Report<bgp::Output, bgp::Failure>,
    indent: &str,
    show_ip: bool,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    if output.ipv4.is_none() && output.ipv6.is_none() {
        return;
    }

    out.line(&format!(
        "{indent}{}",
        words.bgp_method(output.settings.custom_endpoints, output.settings.upload)
    ));

    let sub = format!("{indent}  ");
    let v4 = output.ipv4.as_deref();
    let v6 = output.ipv6.as_deref();

    // 1. Side-by-side IPv4 / IPv6 BGP summary table.
    out.heading(indent, words.bgp_summary_title());
    let summary_headers = words.bgp_summary_headers().map(str::to_owned);
    let summary_rows = build_bgp_summary_rows(v4, v6, show_ip);
    out.table_left(&sub, &summary_headers, &summary_rows);

    // 2. Per-family topology link / Mermaid diagram and top observed AS_PATHs.
    for (ipv6, maybe_rep) in [(false, v4), (true, v6)] {
        let Some(rep) = maybe_rep else {
            continue;
        };
        if !rep.announced || rep.paths.is_empty() {
            continue;
        }
        if out.is_markdown() {
            if let Some(graph) = bgp_graph::layout(rep, Some(bgp_graph::COMPACT_MAX_PATHS)) {
                out.heading(indent, words.bgp_topology_title(ipv6));
                out.code_block("mermaid", &bgp_graph::render_mermaid(&graph));
            }
        } else if let Some(url) = &rep.diagram_url {
            out.field(indent, words.bgp_topology_title(ipv6), url);
        }

        out.heading(indent, words.bgp_paths_title(ipv6));
        let path_headers = words.bgp_paths_headers().map(str::to_owned);
        let path_rows: Vec<Vec<String>> = rep
            .paths
            .iter()
            .take(SHOWN_BGP_PATHS)
            .map(|p| format_bgp_path_row(rep, p))
            .collect();
        out.table(&sub, &path_headers, &path_rows);
    }

    if !out.is_markdown() {
        for (ipv6, reason) in &output.upload_notes {
            out.field(&sub, &words.note(), &words.bgp_upload_failed(*ipv6, reason));
        }
    }
    for (ipv6, err) in &output.notes {
        let reason = format_bgp_query_error(err);
        out.field(
            &sub,
            &words.note(),
            &words.bgp_family_failed(*ipv6, &reason),
        );
    }
}

fn build_bgp_summary_rows(
    v4: Option<&FamilyBgp>,
    v6: Option<&FamilyBgp>,
    show_ip: bool,
) -> Vec<Vec<String>> {
    let words = locale::words();
    let pair = |field: BgpField, f: &dyn Fn(&FamilyBgp) -> String| -> Vec<String> {
        vec![
            words.bgp_field(field).to_owned(),
            v4.map_or_else(|| MISSING.to_owned(), f),
            v6.map_or_else(|| MISSING.to_owned(), f),
        ]
    };

    vec![
        pair(BgpField::Address, &|rep| {
            if show_ip {
                rep.address.to_string()
            } else {
                network::mask(rep.address)
            }
        }),
        pair(BgpField::Prefix, &|rep| {
            if !rep.announced {
                return words.bgp_not_announced().to_owned();
            }
            if show_ip {
                rep.prefix.clone().unwrap_or_else(|| MISSING.to_owned())
            } else if let (Some(base), Some(plen)) = (rep.prefix_base, rep.prefix_len) {
                format!("{}/{plen}", network::mask(base))
            } else {
                MISSING.to_owned()
            }
        }),
        pair(BgpField::OriginAsn, &|rep| {
            if !rep.announced {
                return MISSING.to_owned();
            }
            let Some(asn) = rep.origin_asn else {
                return MISSING.to_owned();
            };
            let name = asn_label(rep, asn);
            words.bgp_origin_cell(asn, name, rep.origins.len())
        }),
        pair(BgpField::Rpki, &|rep| {
            if !rep.announced {
                return MISSING.to_owned();
            }
            let code = match rep.rpki {
                RpkiStatus::Valid => "valid",
                RpkiStatus::Invalid => "invalid",
                RpkiStatus::NotFound => "not_found",
                RpkiStatus::Unknown => "unknown",
            };
            words.bgp_rpki_cell(code).to_owned()
        }),
        pair(BgpField::Visibility, &|rep| {
            if !rep.announced {
                return MISSING.to_owned();
            }
            words.bgp_visibility_cell(rep.total_peers, rep.distinct_paths)
        }),
        pair(BgpField::Upstreams, &|rep| {
            if !rep.announced {
                return MISSING.to_owned();
            }
            if rep.upstreams.is_empty() {
                return words.bgp_direct_peering().to_owned();
            }
            let total = rep.total_peers.max(1);
            let mut items = Vec::new();
            let shown = rep.upstreams.iter().take(SHOWN_UPSTREAMS);
            for up in shown {
                let pct = (u64::from(up.peers) * 100 + u64::from(total) / 2) / u64::from(total);
                let item = match asn_label(rep, up.asn) {
                    Some(name) => format!("AS{} {name} ({pct}%)", up.asn),
                    None => format!("AS{} ({pct}%)", up.asn),
                };
                items.push(item);
            }
            if rep.upstreams.len() > SHOWN_UPSTREAMS {
                items.push(format!("+{}", rep.upstreams.len() - SHOWN_UPSTREAMS));
            }
            items.join(", ")
        }),
    ]
}

fn format_bgp_path_row(rep: &FamilyBgp, path: &BgpPath) -> Vec<String> {
    let raw = if path.asns_raw.is_empty() {
        &path.asns
    } else {
        &path.asns_raw
    };
    let mut collapsed: Vec<(u32, usize)> = Vec::new();
    for &asn in raw {
        if let Some(last) = collapsed.last_mut()
            && last.0 == asn
        {
            last.1 += 1;
            continue;
        }
        collapsed.push((asn, 1));
    }
    let mut hops = Vec::with_capacity(collapsed.len());
    for (asn, rep_count) in collapsed {
        let star = if asns::is_tier1(asn) { " ★" } else { "" };
        let mult = if rep_count > 1 {
            format!(" ×{rep_count}")
        } else {
            String::new()
        };
        let label = match asn_label(rep, asn) {
            Some(name) => format!("AS{asn}{star}{mult} ({name})"),
            None => format!("AS{asn}{star}{mult}"),
        };
        hops.push(label);
    }
    let prefix_mark = if path.is_best { "★ " } else { "" };
    vec![
        path.peers.to_string(),
        path.asns.len().to_string(),
        format!("{prefix_mark}{}", hops.join(" -> ")),
    ]
}

fn asn_label(rep: &FamilyBgp, asn: u32) -> Option<&str> {
    rep.asn_names
        .get(&asn)
        .map(String::as_str)
        .or_else(|| asns::known_name(asn))
}

/// Why `network.bgp` stopped short.
pub(super) fn bgp_failure(failure: &bgp::Failure) -> String {
    let words = locale::words();
    match failure {
        bgp::Failure::InvalidEndpoint(raw) => words.ip_invalid_endpoint(raw),
        bgp::Failure::NoFamily => words.no_family(),
        bgp::Failure::NoPublicAddress => words.ip_no_public_address(),
        bgp::Failure::QueryFailed(err) => words.bgp_query_failed(&format_bgp_query_error(err)),
        bgp::Failure::Cancelled => words.interrupted(),
    }
}

fn format_bgp_query_error(err: &bgp::result::QueryError) -> String {
    use bgp::result::QueryError as E;

    let issue = match err {
        E::NoAddress => BgpIssueRef::NoAddress,
        E::TimedOut => BgpIssueRef::TimedOut,
        E::TooManyRedirects => BgpIssueRef::TooManyRedirects,
        E::InvalidRedirect(why) => BgpIssueRef::InvalidRedirect(why),
        E::NotHttps(why) => BgpIssueRef::NotHttps(why),
        E::Transport(why) => BgpIssueRef::Transport(why),
        E::HttpStatus(status) => BgpIssueRef::HttpStatus(*status),
        E::InvalidJson(why) => BgpIssueRef::InvalidJson(why),
        E::ApiStatus(status) => BgpIssueRef::ApiStatus(status),
        E::MissingData => BgpIssueRef::MissingData,
        E::NoData => BgpIssueRef::NoData,
    };
    locale::words().bgp_issue(issue)
}

/// Cloudflare speed (`network.cloudflare`): method line, IPv4 / IPv6 side-by-side summary table
/// (masking `client_ip` unless `show_ip`), and per-family UDP or failure notes.
pub(super) fn cloudflare(
    out: &mut Out,
    report: &Report<cloudflare::Output, cloudflare::Failure>,
    indent: &str,
    show_ip: bool,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let settings = output.settings;
    let choice = CloudflareChoice {
        concurrency: settings.concurrency,
        seconds: settings.seconds,
        udp: settings.udp,
        udp_packets: settings.udp_packets,
        custom_endpoints: settings.custom_endpoints,
    };
    out.line(&format!("{indent}{}", words.cloudflare_method(&choice)));

    let v4 = output.ipv4.as_deref();
    let v6 = output.ipv6.as_deref();
    if v4.is_some() || v6.is_some() {
        let headers = words.cloudflare_headers().map(str::to_owned);
        let rows = build_cloudflare_rows(v4, v6, settings.udp, show_ip);
        out.table_left(indent, &headers, &rows);
    }

    for (ipv6, maybe_fam) in [(false, v4), (true, v6)] {
        if let Some(fam) = maybe_fam
            && let Some(issue) = &fam.udp_note
        {
            let reason = words.cloudflare_issue(cloudflare_issue_ref(issue));
            out.field(
                indent,
                &words.note(),
                &words.cloudflare_udp_note(ipv6, &reason),
            );
        }
    }
    for (ipv6, issue) in &output.notes {
        let reason = words.cloudflare_issue(cloudflare_issue_ref(issue));
        out.field(
            indent,
            &words.note(),
            &words.cloudflare_family_failed(*ipv6, &reason),
        );
    }
}

fn cloudflare_issue_ref(issue: &cloudflare::FamilyIssue) -> CloudflareIssueRef<'_> {
    match issue {
        cloudflare::FamilyIssue::EmptyDownload => CloudflareIssueRef::EmptyDownload,
        cloudflare::FamilyIssue::EmptyUpload => CloudflareIssueRef::EmptyUpload,
        cloudflare::FamilyIssue::NoAddress => CloudflareIssueRef::NoAddress,
        cloudflare::FamilyIssue::TimedOut => CloudflareIssueRef::TimedOut,
        cloudflare::FamilyIssue::NoResolver => CloudflareIssueRef::NoResolver,
        cloudflare::FamilyIssue::Transport(why) => CloudflareIssueRef::Transport(why),
    }
}

fn build_cloudflare_rows(
    v4: Option<&FamilyCloudflare>,
    v6: Option<&FamilyCloudflare>,
    show_udp: bool,
    show_ip: bool,
) -> Vec<Vec<String>> {
    let words = locale::words();
    let pair = |field: CloudflareField, f: &dyn Fn(&FamilyCloudflare) -> String| -> Vec<String> {
        vec![
            words.cloudflare_field(field).to_owned(),
            v4.map_or_else(|| MISSING.to_owned(), f),
            v6.map_or_else(|| MISSING.to_owned(), f),
        ]
    };

    let fmt_lat = |lat: &cloudflare::LatencyStats| -> String {
        let (Some(med), Some(min), Some(max)) = (lat.median_ms, lat.min_ms, lat.max_ms) else {
            return MISSING.to_owned();
        };
        let med_s = format_ms(med);
        let min_s = format_ms(min);
        let max_s = format_ms(max);
        let jit_s = lat.jitter_ms.map(format_ms);
        words.cloudflare_latency_cell(&med_s, &min_s, &max_s, jit_s.as_deref(), lat.loss_pct)
    };

    let mut rows = vec![
        pair(CloudflareField::Server, &|fam| {
            fam.server
                .clone()
                .or_else(|| fam.colo.clone())
                .unwrap_or_else(|| MISSING.to_owned())
        }),
        pair(CloudflareField::ClientIp, &|fam| match fam.client_ip {
            Some(ip) if show_ip => ip.to_string(),
            Some(ip) => network::mask(ip),
            None => MISSING.to_owned(),
        }),
        pair(
            CloudflareField::ClientAsn,
            &|fam| match (fam.asn, fam.as_org.as_deref()) {
                (Some(asn), Some(org)) => format!("AS{asn} ({org})"),
                (Some(asn), None) => format!("AS{asn}"),
                (None, Some(org)) => org.to_owned(),
                (None, None) => MISSING.to_owned(),
            },
        ),
        pair(CloudflareField::IdleLatency, &|fam| {
            fmt_lat(&fam.idle_latency)
        }),
        pair(CloudflareField::Download, &|fam| {
            words.cloudflare_speed_cell(
                &bits(fam.download.bps),
                fam.download.median_mbps,
                fam.download.p95_mbps,
            )
        }),
        pair(CloudflareField::DownloadLatency, &|fam| {
            fmt_lat(&fam.loaded_download_latency)
        }),
        pair(CloudflareField::Upload, &|fam| {
            words.cloudflare_speed_cell(
                &bits(fam.upload.bps),
                fam.upload.median_mbps,
                fam.upload.p95_mbps,
            )
        }),
        pair(CloudflareField::UploadLatency, &|fam| {
            fmt_lat(&fam.loaded_upload_latency)
        }),
    ];

    if show_udp || v4.is_some_and(|f| f.udp.is_some()) || v6.is_some_and(|f| f.udp.is_some()) {
        rows.push(pair(CloudflareField::UdpLoss, &|fam| match &fam.udp {
            Some(u) => words.cloudflare_udp_cell(
                u.loss_pct,
                u.received,
                u.sent,
                u.out_of_order_pct,
                u.mos,
                words.cloudflare_udp_quality(u.quality.code()),
            ),
            None => MISSING.to_owned(),
        }));
    }

    rows.push(pair(CloudflareField::Quality, &|fam| match &fam.quality {
        Some(q) => words.cloudflare_quality_cell(
            q.bufferbloat_grade.label(),
            q.bufferbloat_ms,
            q.stability_grade.label(),
            q.stability_cv_pct,
        ),
        None => MISSING.to_owned(),
    }));
    rows
}

fn format_ms(ms: f64) -> String {
    format!("{} ms", digits(ms.max(0.0)))
}

/// Why `network.cloudflare` stopped short.
pub(super) fn cloudflare_failure(failure: &cloudflare::Failure) -> String {
    let words = locale::words();
    match failure {
        cloudflare::Failure::InvalidEndpoint(raw) => words.ip_invalid_endpoint(raw),
        cloudflare::Failure::NoFamily => words.no_family(),
        cloudflare::Failure::NoResult(issue) => {
            let reason = issue.as_ref().map_or_else(String::new, |i| {
                words.cloudflare_issue(cloudflare_issue_ref(i))
            });
            words.cloudflare_no_result(&reason)
        }
        cloudflare::Failure::Cancelled => words.interrupted(),
    }
}

/// Apple CDN speed (`network.inetspeed`): method line, selected endpoint & idle latency fields,
/// 4-round throughput & loaded-latency table, and any degradation notes.
pub(super) fn inetspeed(
    out: &mut Out,
    report: &Report<inetspeed::Output, inetspeed::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let settings = &output.settings;
    let choice = InetspeedChoice {
        threads: settings.threads,
        timeout_secs: settings.timeout_secs,
        max: settings.max.clone(),
        latency_count: settings.latency_count,
        forced_endpoint: settings.forced_endpoint.is_some(),
        custom_endpoints: settings.custom_endpoints,
    };
    out.line(&format!("{indent}{}", words.inetspeed_method(&choice)));

    let ep_ip = output.selected.ip.map(|ip| ip.to_string());
    let ep_val = words.inetspeed_endpoint_value(
        &output.selected.host,
        ep_ip.as_deref(),
        output.selected.rtt_ms,
        output.selected.source.code(),
    );
    out.field(indent, words.inetspeed_endpoint_label(), &ep_val);

    if let Some(idle) = &output.idle_latency {
        out.field(
            indent,
            words.inetspeed_idle_label(),
            &words.inetspeed_latency_value(
                idle.median_ms,
                idle.min_ms,
                idle.avg_ms,
                idle.max_ms,
                idle.jitter_ms,
            ),
        );
    }

    out.field(
        indent,
        words.inetspeed_total_label(),
        &human_bytes(output.total_bytes),
    );

    if !output.rounds.is_empty() {
        let headers = words.inetspeed_headers().map(str::to_owned);
        let rows: Vec<Vec<String>> = output.rounds.iter().map(inetspeed_row).collect();
        out.table_left(indent, &headers, &rows);
    }

    for round in &output.rounds {
        if let Some(issue) = &round.error {
            let label = words.inetspeed_round_name(
                round.direction == TransferDirection::Download,
                round.threads <= 1,
            );
            let detail = words.inetspeed_round_issue(inetspeed_round_issue_ref(issue));
            out.field(indent, &words.note(), &format!("{label}: {detail}"));
        }
    }
    for warning in &output.warnings {
        let text = match warning {
            inetspeed::Warning::HostMismatch => {
                words.inetspeed_warning(InetspeedWarningRef::HostMismatch)
            }
            inetspeed::Warning::ForcedProbeFailed(ip) => {
                let ip_s = ip.to_string();
                words.inetspeed_warning(InetspeedWarningRef::ForcedProbeFailed(&ip_s))
            }
            inetspeed::Warning::DohEmpty => words.inetspeed_warning(InetspeedWarningRef::DohEmpty),
            inetspeed::Warning::UnpinnedFallback => {
                words.inetspeed_warning(InetspeedWarningRef::UnpinnedFallback)
            }
            inetspeed::Warning::NoIdleSamples => {
                words.inetspeed_warning(InetspeedWarningRef::NoIdleSamples)
            }
            inetspeed::Warning::IdleFailed(fault) => {
                words.inetspeed_warning(InetspeedWarningRef::IdleFailed(inetspeed_fault_ref(fault)))
            }
        };
        out.field(indent, &words.note(), &text);
    }
}

fn inetspeed_fault_ref(fault: &inetspeed::ProbeFault) -> InetspeedProbeFaultRef<'_> {
    match fault {
        inetspeed::ProbeFault::EmptySample => InetspeedProbeFaultRef::EmptySample,
        inetspeed::ProbeFault::NoAddress => InetspeedProbeFaultRef::NoAddress,
        inetspeed::ProbeFault::TimedOut => InetspeedProbeFaultRef::TimedOut,
        inetspeed::ProbeFault::NoResolver => InetspeedProbeFaultRef::NoResolver,
        inetspeed::ProbeFault::Transport(why) => InetspeedProbeFaultRef::Transport(why),
    }
}

fn inetspeed_round_issue_ref(issue: &inetspeed::RoundIssue) -> InetspeedRoundIssueRef<'_> {
    match issue {
        inetspeed::RoundIssue::ZeroBytes => InetspeedRoundIssueRef::ZeroBytes,
        inetspeed::RoundIssue::WorkerFaults(count) => InetspeedRoundIssueRef::WorkerFaults(*count),
        inetspeed::RoundIssue::Transport(fault) => {
            InetspeedRoundIssueRef::Transport(inetspeed_fault_ref(fault))
        }
    }
}

fn inetspeed_row(round: &RoundResult) -> Vec<String> {
    let words = locale::words();
    let name = words
        .inetspeed_round_name(
            round.direction == TransferDirection::Download,
            round.threads <= 1,
        )
        .to_owned();
    let speed = if round.bytes == 0 {
        words.way_cell(WayCell::Error).to_owned()
    } else {
        bits(round.bps)
    };
    let data = human_bytes(round.bytes);
    let loaded = round.loaded_latency.map_or_else(
        || MISSING.to_owned(),
        |lat| words.inetspeed_loaded_cell(lat.median_ms, lat.jitter_ms),
    );
    vec![name, round.threads.to_string(), speed, data, loaded]
}

fn human_bytes(bytes: u64) -> String {
    const GIB: f64 = 1_073_741_824.0;
    const MIB: f64 = 1_048_576.0;
    const KIB: f64 = 1_024.0;
    let b = bytes as f64;
    if bytes >= (1_u64 << 30) {
        format!("{:.2} GiB", b / GIB)
    } else if bytes >= (1_u64 << 20) {
        format!("{:.1} MiB", b / MIB)
    } else if bytes >= (1_u64 << 10) {
        format!("{:.0} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

/// Why `network.inetspeed` stopped short.
pub(super) fn inetspeed_failure(failure: &inetspeed::Failure) -> String {
    let words = locale::words();
    match failure {
        inetspeed::Failure::InvalidEndpoint(raw) => words.ip_invalid_endpoint(raw),
        inetspeed::Failure::InvalidMax(raw) => words.inetspeed_invalid_max(raw),
        inetspeed::Failure::NoFamily => words.no_family(),
        inetspeed::Failure::NoResult => words.inetspeed_no_result(),
        inetspeed::Failure::Cancelled => words.interrupted(),
    }
}
