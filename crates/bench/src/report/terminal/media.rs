//! The streaming media section: regional tables comparing IPv4 and IPv6 unlock verdicts, with
//! notes on unknown service ids and unexpected responses.

use plugin_media::unlock;
use plugin_media::unlock::Group;
use plugin_media::unlock::Restriction;
use plugin_media::unlock::ServiceResult;
use plugin_media::unlock::Verdict;

use super::Out;
use crate::locale;
use crate::locale::MediaCell;
use crate::locale::MediaChoice;
use crate::results::Report;

/// Missing or unreachable family cell.
const MISSING: &str = "—";

/// Renders `media.unlock`'s block inside `[Streaming media]`.
pub(super) fn unlock(
    out: &mut Out,
    report: &Report<unlock::Output, unlock::Failure>,
    indent: &str,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    let choice = MediaChoice {
        groups: output
            .settings
            .groups
            .iter()
            .map(|item| item.code().to_owned())
            .collect(),
        services: output.settings.services.clone(),
        custom_base: output.settings.custom_base,
    };
    out.line(&format!("{indent}{}", words.media_method(&choice)));
    if !output.unknown.is_empty() {
        out.field(
            indent,
            &words.note(),
            &words.unknown_services(&output.unknown),
        );
    }
    let headers = words.media_headers().map(str::to_owned);
    for group in Group::ALL {
        let members: Vec<&ServiceResult> = output
            .results
            .iter()
            .filter(|service| service.group == group)
            .collect();
        if members.is_empty() {
            continue;
        }
        out.line(&format!("{indent}{}", words.media_group(group.code())));
        let sub = format!("{indent}  ");
        let rows: Vec<Vec<String>> = members
            .iter()
            .map(|service| row(service, output.ipv4_reachable, output.ipv6_reachable))
            .collect();
        out.table_left(&sub, &headers, &rows);
        for service in &members {
            for (ipv6, verdict) in [(false, &service.ipv4), (true, &service.ipv6)] {
                if let Some(Verdict::Unexpected(reason)) = verdict {
                    out.field(
                        &sub,
                        &words.note(),
                        &words.media_unexpected(service.name, ipv6, reason),
                    );
                }
            }
        }
    }
}

fn row(service: &ServiceResult, ipv4_reachable: bool, ipv6_reachable: bool) -> Vec<String> {
    vec![
        service.name.to_owned(),
        cell(service.ipv4.as_ref(), ipv4_reachable, false),
        cell(service.ipv6.as_ref(), ipv6_reachable, true),
    ]
}

fn cell(verdict: Option<&Verdict>, reachable: bool, ipv6: bool) -> String {
    if !reachable {
        return MISSING.to_owned();
    }
    let Some(verdict) = verdict else {
        return MISSING.to_owned();
    };
    let words = locale::words();
    match verdict {
        Verdict::Yes { region } => words.media_yes(region.as_deref()),
        Verdict::OriginalsOnly { region } => words.media_originals(region.as_deref()),
        Verdict::No { restriction } => {
            let kind = match restriction {
                None => MediaCell::No,
                Some(Restriction::ChinaMainland) => MediaCell::NoChina,
                Some(Restriction::ComingSoon) => MediaCell::ComingSoon,
                Some(Restriction::ProxyDetected) => MediaCell::ProxyDetected,
            };
            words.media_cell(kind, ipv6).to_owned()
        }
        Verdict::BlockedWaf => words.media_cell(MediaCell::BlockedWaf, ipv6).to_owned(),
        Verdict::RateLimited => words.media_cell(MediaCell::RateLimited, ipv6).to_owned(),
        Verdict::NoAddress => words.media_cell(MediaCell::NoAddress, ipv6).to_owned(),
        Verdict::TimedOut => words.media_cell(MediaCell::TimedOut, ipv6).to_owned(),
        Verdict::Unexpected(_) => words.media_cell(MediaCell::Error, ipv6).to_owned(),
    }
}

/// Formats `media.unlock`'s stop reason.
pub(super) fn unlock_failure(failure: &unlock::Failure) -> String {
    let words = locale::words();
    match failure {
        unlock::Failure::InvalidBaseUrl(url) => words.invalid_base_url(url),
        unlock::Failure::NoService(unknown) => words.no_service(unknown),
        unlock::Failure::NoFamily => words.no_media_family(),
        unlock::Failure::Cancelled => words.interrupted(),
    }
}
