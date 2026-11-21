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
            Way::Fault(job::Fault::TimedOut(_)) => continue,
            Way::Fault(fault) => tool::fault("iperf3", fault, Unit::Seconds),
            Way::Nothing => words.nothing_measured(),
            Way::Measured(_) | Way::Busy | Way::Unreachable | Way::Skipped => continue,
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
        other => {
            let kind = match other {
                NodeOutcome::Busy => TaierCell::Busy,
                NodeOutcome::Unreachable => TaierCell::Unreachable,
                NodeOutcome::AuthRejected => TaierCell::AuthRejected,
                NodeOutcome::TimedOut => TaierCell::TimedOut,
                NodeOutcome::Failed(_) => TaierCell::Error,
                NodeOutcome::Measured(_) => unreachable!(),
            };
            let text = words.taier_cell(kind).to_owned();
            (text.clone(), text)
        }
    };
    vec![place, operator, down, up]
}

/// Why `network.taier` stopped short.
pub(super) fn taier_failure(failure: &taier::Failure) -> String {
    let words = locale::words();
    match failure {
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
    for (ipv6, traced) in [(false, &output.ipv4), (true, &output.ipv6)] {
        if traced.is_empty() {
            continue;
        }
        out.heading(indent, words.family_title(ipv6));
        for target in traced {
            let outcome = match &target.route {
                Route::Traced(trace) if trace.reached => words.reached(trace.hops.len()),
                Route::Traced(trace) => words.not_reached(trace.hops.len()),
                Route::Unavailable(why) => words.unavailable_lookup(&unavailable(why)),
                Route::Failed(error) => trace_error(error),
            };
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
/// network with its exchange, its place and its latency; `*` for a hop that no probe got an
/// answer from. A second address of a hop takes a row of its own, under the same number.
fn hop_rows(trace: &Trace, own: &[IpAddr], show_ip: bool) -> Vec<Vec<String>> {
    let words = locale::words();
    let mut rows = Vec::new();
    for (number, hop) in (1_usize..).zip(&trace.hops) {
        if hop.answers.is_empty() {
            let mut row = vec![number.to_string(), "*".to_owned()];
            row.resize(5, String::new());
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
            let network = match &answer.exchange {
                Some(exchange) => words.at_exchange(answer.network.as_deref(), exchange),
                None => answer.network.clone().unwrap_or_else(|| MISSING.to_owned()),
            };
            let place = answer.place.clone().unwrap_or_else(|| MISSING.to_owned());
            rows.push(vec![hop, address, network, place, latency(answer.latency)]);
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
    for (ipv6, reason) in &output.notes {
        out.field(&sub, &words.note(), &words.bgp_family_failed(*ipv6, reason));
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
            for up in rep.upstreams.iter().take(SHOWN_UPSTREAMS) {
                let pct = ((f64::from(up.peers) * 100.0) / f64::from(total)).round() as u32;
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
        bgp::Failure::QueryFailed(reason) => words.bgp_query_failed(reason),
        bgp::Failure::Cancelled => words.interrupted(),
    }
}
