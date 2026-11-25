//! The IP group's chapter (`[IP]`): IP reputation (`ip.quality`), with side-by-side IPv4 and IPv6
//! tables for network identity, risk scores, risk factors, DNSBL blacklists and port-25 mail;
//! streaming media unlock (`ip.media`), with regional OTT availability tables; and why either item
//! stopped short.

use std::net::IpAddr;

use plugin_ip::media;
use plugin_ip::media::Group;
use plugin_ip::media::Restriction;
use plugin_ip::media::ServiceResult;
use plugin_ip::media::UnlockMode;
use plugin_ip::media::Verdict;
use plugin_ip::quality;
use plugin_ip::quality::DnsblSummary;
use plugin_ip::quality::FactorSummary;
use plugin_ip::quality::FamilyReport;
use plugin_ip::quality::GreyNoiseClass;
use plugin_ip::quality::MailCheck;
use plugin_ip::quality::MailStatus;
use plugin_ip::quality::Origin;
use plugin_ip::quality::RiskFactor;
use plugin_ip::quality::RiskLevel;
use plugin_ip::quality::ScoreVerdict;
use plugin_ip::quality::UsageType;

use super::Out;
use crate::locale;
use crate::locale::IpChoice;
use crate::locale::IpField;
use crate::locale::IpTable;
use crate::locale::MediaCell;
use crate::locale::MediaChoice;
use crate::network;
use crate::results::Report;

/// Cell shown when an IP family is unreachable or a source does not cover it.
const MISSING: &str = "—";

/// Score providers in table order.
const SCORE_SOURCES: [&str; 5] = [
    "Scamalytics",
    "ProxyCheck",
    "ipapi.is",
    "GreyNoise",
    "Shodan",
];

/// Renders `ip.quality`'s block inside `[IP reputation]`.
pub(super) fn quality(
    out: &mut Out,
    report: &Report<quality::Output, quality::Failure>,
    indent: &str,
    own: &[IpAddr],
    show_ip: bool,
) {
    let words = locale::words();
    let Some(output) = &report.output else {
        return;
    };
    if output.ipv4.is_none() && output.ipv6.is_none() {
        return;
    }
    let choice = IpChoice {
        dnsbl: output.settings.dnsbl,
        mail: output.settings.mail,
        custom_endpoints: output.settings.custom_endpoints,
    };
    out.line(&format!("{indent}{}", words.ip_method(&choice)));

    let sub = format!("{indent}  ");
    let v4 = output.ipv4.as_deref();
    let v6 = output.ipv6.as_deref();

    // 1. Summary table.
    out.heading(indent, words.ip_table(IpTable::Summary));
    let summary_headers = words.ip_headers(IpTable::Summary).map(str::to_owned);
    let summary_rows = build_summary_rows(v4, v6, show_ip);
    out.table_left(&sub, &summary_headers, &summary_rows);

    // 2. Risk scores table.
    out.heading(indent, words.ip_table(IpTable::Scores));
    let score_headers = words.ip_headers(IpTable::Scores).map(str::to_owned);
    let score_rows = build_score_rows(v4, v6);
    out.table_left(&sub, &score_headers, &score_rows);

    // 3. Risk factors table.
    out.heading(indent, words.ip_table(IpTable::Factors));
    let factor_headers = words.ip_headers(IpTable::Factors).map(str::to_owned);
    let factor_rows = build_factor_rows(v4, v6);
    out.table_left(&sub, &factor_headers, &factor_rows);

    // 4. Blacklists & mail (port 25) table.
    if output.settings.dnsbl || output.settings.mail {
        out.heading(indent, words.ip_table(IpTable::BlacklistsAndMail));
        let bm_headers = words
            .ip_headers(IpTable::BlacklistsAndMail)
            .map(str::to_owned);
        let bm_rows =
            build_blacklist_and_mail_rows(v4, v6, output.settings.dnsbl, output.settings.mail);
        out.table_left(&sub, &bm_headers, &bm_rows);
    }

    // Unexpected provider notes.
    let mut redact_addrs = own.to_vec();
    for maybe_rep in [v4, v6] {
        if let Some(rep) = maybe_rep
            && !redact_addrs.contains(&rep.address)
        {
            redact_addrs.push(rep.address);
        }
    }
    for (ipv6, fam) in [(false, v4), (true, v6)] {
        if let Some(rep) = fam {
            for (provider, reason) in &rep.notes {
                let shown_reason = if show_ip {
                    reason.clone()
                } else {
                    redact_ip_in_note(reason, &redact_addrs)
                };
                out.field(
                    &sub,
                    &words.note(),
                    &words.ip_unexpected(provider, ipv6, &shown_reason),
                );
            }
        }
    }
}

pub(in crate::report) fn redact_ip_in_note(reason: &str, addrs: &[IpAddr]) -> String {
    let mut text = reason.to_owned();
    for &addr in addrs {
        let masked = network::mask(addr);
        match addr {
            IpAddr::V4(v4) => {
                let raw = v4.to_string();
                let encoded = raw.replace('.', "%2E");
                text = replace_ip_token(&text, &encoded, &masked, false);
                text = replace_ip_token(&text, &raw, &masked, false);
            }
            IpAddr::V6(v6) => {
                let segs = v6.segments();
                let padded = format!(
                    "{:04x}:{:04x}:{:04x}:{:04x}:{:04x}:{:04x}:{:04x}:{:04x}",
                    segs[0], segs[1], segs[2], segs[3], segs[4], segs[5], segs[6], segs[7]
                );
                let full = format!(
                    "{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
                    segs[0], segs[1], segs[2], segs[3], segs[4], segs[5], segs[6], segs[7]
                );
                let canonical = v6.to_string();
                for candidate in [
                    padded.replace(':', "%3A"),
                    padded,
                    full.replace(':', "%3A"),
                    full,
                    canonical.replace(':', "%3A"),
                    canonical,
                ] {
                    text = replace_ip_token(&text, &candidate, &masked, true);
                }
            }
        }
    }
    text
}

fn replace_ip_token(haystack: &str, needle: &str, replacement: &str, ipv6: bool) -> String {
    if needle.is_empty() || haystack.len() < needle.len() {
        return haystack.to_owned();
    }
    let is_token_cont = |b: u8| {
        if ipv6 {
            b.is_ascii_hexdigit() || b == b':'
        } else {
            b.is_ascii_digit()
        }
    };
    let sep_hex: &[u8; 2] = if ipv6 { b"3A" } else { b"2E" };
    let bytes = haystack.as_bytes();
    let nlen = needle.len();
    let mut out = String::with_capacity(haystack.len());
    let mut cursor = 0;
    let mut i = 0;
    while i + nlen <= bytes.len() {
        if haystack.is_char_boundary(i)
            && haystack.is_char_boundary(i + nlen)
            && haystack[i..i + nlen].eq_ignore_ascii_case(needle)
        {
            let pct_before = i >= 3
                && bytes[i - 3] == b'%'
                && bytes[i - 2].is_ascii_hexdigit()
                && bytes[i - 1].is_ascii_hexdigit()
                && !bytes[i - 2..i].eq_ignore_ascii_case(sep_hex);
            let prev_ok = i == 0 || !is_token_cont(bytes[i - 1]) || pct_before;
            let pct_sep_after = i + nlen + 3 <= bytes.len()
                && bytes[i + nlen] == b'%'
                && bytes[i + nlen + 1..i + nlen + 3].eq_ignore_ascii_case(sep_hex);
            let next_ok =
                (i + nlen == bytes.len() || !is_token_cont(bytes[i + nlen])) && !pct_sep_after;
            if prev_ok && next_ok {
                out.push_str(&haystack[cursor..i]);
                out.push_str(replacement);
                i += nlen;
                cursor = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&haystack[cursor..]);
    out
}

fn build_summary_rows(
    v4: Option<&FamilyReport>,
    v6: Option<&FamilyReport>,
    show_ip: bool,
) -> Vec<Vec<String>> {
    let words = locale::words();
    let pair = |field: IpField, f: &dyn Fn(&FamilyReport) -> String| -> Vec<String> {
        vec![
            words.ip_field(field).to_owned(),
            v4.map_or_else(|| MISSING.to_owned(), f),
            v6.map_or_else(|| MISSING.to_owned(), f),
        ]
    };

    let mut rows = vec![pair(IpField::Address, &|rep| {
        if show_ip {
            rep.address.to_string()
        } else {
            network::mask(rep.address)
        }
    })];

    if show_ip
        && (v4.and_then(|r| r.hostname.as_ref()).is_some()
            || v6.and_then(|r| r.hostname.as_ref()).is_some())
    {
        rows.push(pair(IpField::Hostname, &|rep| {
            rep.hostname.clone().unwrap_or_else(|| MISSING.to_owned())
        }));
    }

    rows.push(pair(IpField::Asn, &|rep| {
        rep.asn
            .map_or_else(|| words.unknown(), |asn| format!("AS{asn}"))
    }));
    rows.push(pair(IpField::Organization, &|rep| {
        rep.org.clone().unwrap_or_else(|| words.unknown())
    }));
    rows.push(pair(IpField::Registry, &registry_cell));
    rows.push(pair(IpField::Origin, &origin_cell));
    rows.push(pair(IpField::Location, &location_cell));
    rows.push(pair(IpField::Timezone, &|rep| {
        rep.timezone.clone().unwrap_or_else(|| words.unknown())
    }));
    rows.push(pair(IpField::AsnType, &asn_type_cell));
    rows.push(pair(IpField::CompanyType, &company_type_cell));
    rows.push(pair(IpField::GeoIp, &geo_cell));
    rows
}

fn location_cell(rep: &FamilyReport) -> String {
    match (rep.city.as_deref(), rep.region.as_deref()) {
        (Some(city), Some(region)) if city.eq_ignore_ascii_case(region) => city.to_owned(),
        (Some(city), Some(region)) => format!("{city}, {region}"),
        (Some(city), None) => city.to_owned(),
        (None, Some(region)) => region.to_owned(),
        (None, None) => locale::words().unknown(),
    }
}

fn asn_type_cell(rep: &FamilyReport) -> String {
    let words = locale::words();
    let Some(primary) = rep.asn_usage else {
        return words.unknown();
    };
    let primary_label = words.ip_usage(usage_code(primary));
    let mut diffs = Vec::new();
    for entry in &rep.usage_sources {
        if let Some(other) = entry.asn_usage
            && other != primary
        {
            diffs.push(format!(
                "{}: {}",
                entry.source,
                words.ip_usage(usage_code(other))
            ));
        }
    }
    if diffs.is_empty() {
        primary_label.to_owned()
    } else {
        format!("{primary_label} ({})", diffs.join(", "))
    }
}

fn company_type_cell(rep: &FamilyReport) -> String {
    let words = locale::words();
    let Some(comp) = rep.company_usage else {
        return words.unknown();
    };
    let label = words.ip_usage(usage_code(comp));
    if rep.dual_isp == Some(true) {
        words.ip_dual_isp(label)
    } else {
        label.to_owned()
    }
}

fn registry_cell(rep: &FamilyReport) -> String {
    match (&rep.rir, &rep.registered_country) {
        (Some(rir), Some(cc)) => format!("{rir} ({cc})"),
        (Some(rir), None) => rir.clone(),
        (None, Some(cc)) => cc.clone(),
        (None, None) => locale::words().unknown(),
    }
}

fn origin_cell(rep: &FamilyReport) -> String {
    let words = locale::words();
    match &rep.origin {
        Origin::Native { country } => words.ip_native(country),
        Origin::Broadcast {
            registered,
            located,
        } => words.ip_broadcast(registered, located),
        Origin::Unknown => words.unknown(),
    }
}

fn geo_cell(rep: &FamilyReport) -> String {
    if rep.geo_votes.is_empty() || rep.geo_total == 0 {
        return locale::words().unknown();
    }
    let parts: Vec<String> = rep
        .geo_votes
        .iter()
        .map(|vote| format!("{} ({}/{})", vote.country, vote.count, rep.geo_total))
        .collect();
    parts.join(", ")
}

fn build_score_rows(v4: Option<&FamilyReport>, v6: Option<&FamilyReport>) -> Vec<Vec<String>> {
    SCORE_SOURCES
        .into_iter()
        .map(|source| {
            vec![
                source.to_owned(),
                score_for(v4, source, false),
                score_for(v6, source, true),
            ]
        })
        .collect()
}

fn score_for(rep: Option<&FamilyReport>, source: &str, ipv6: bool) -> String {
    let Some(rep) = rep else {
        return MISSING.to_owned();
    };
    let Some((_, verdict)) = rep.scores.iter().find(|(name, _)| *name == source) else {
        return MISSING.to_owned();
    };
    let words = locale::words();
    match verdict {
        ScoreVerdict::Numeric { score, max, level } => {
            words.ip_numeric_score(*score, *max, level_code(*level))
        }
        ScoreVerdict::Ratio { text, level } => words.ip_ratio_score(text, level_code(*level)),
        ScoreVerdict::GreyNoise {
            riot,
            noise,
            classification,
        } => words
            .ip_greynoise(*riot, *noise, greynoise_code(*classification))
            .to_owned(),
        ScoreVerdict::Shodan {
            ports,
            vulns,
            flagged,
        } => words.ip_shodan(*ports, *vulns, *flagged),
        ScoreVerdict::BlockedWaf => words.media_cell(MediaCell::BlockedWaf, ipv6).to_owned(),
        ScoreVerdict::RateLimited => words.media_cell(MediaCell::RateLimited, ipv6).to_owned(),
        ScoreVerdict::NoAddress => MISSING.to_owned(),
        ScoreVerdict::TimedOut => words.media_cell(MediaCell::TimedOut, ipv6).to_owned(),
        ScoreVerdict::Unexpected(_) => words.media_cell(MediaCell::Error, ipv6).to_owned(),
    }
}

fn build_factor_rows(v4: Option<&FamilyReport>, v6: Option<&FamilyReport>) -> Vec<Vec<String>> {
    let words = locale::words();
    RiskFactor::ALL
        .into_iter()
        .map(|factor| {
            vec![
                words.ip_factor_name(factor_code(factor)).to_owned(),
                factor_for(v4, factor),
                factor_for(v6, factor),
            ]
        })
        .collect()
}

fn factor_for(rep: Option<&FamilyReport>, factor: RiskFactor) -> String {
    let Some(rep) = rep else {
        return MISSING.to_owned();
    };
    let Some(FactorSummary {
        checked,
        flagged_by,
        ..
    }) = rep.factors.iter().find(|f| f.factor == factor)
    else {
        return MISSING.to_owned();
    };
    if *checked == 0 && flagged_by.is_empty() {
        return MISSING.to_owned();
    }
    locale::words().ip_factor_cell(*checked, flagged_by)
}

fn build_blacklist_and_mail_rows(
    v4: Option<&FamilyReport>,
    v6: Option<&FamilyReport>,
    dnsbl_on: bool,
    mail_on: bool,
) -> Vec<Vec<String>> {
    let words = locale::words();
    let mut rows = Vec::new();
    if dnsbl_on {
        rows.push(vec![
            words.ip_dnsbl_label().to_owned(),
            dnsbl_cell(v4.and_then(|r| r.dnsbl.as_ref())),
            dnsbl_cell(v6.and_then(|r| r.dnsbl.as_ref())),
        ]);
    }
    if mail_on {
        rows.push(vec![
            words.ip_port25_label().to_owned(),
            port25_cell(v4.map(|r| r.mail.as_slice())),
            port25_cell(v6.map(|r| r.mail.as_slice())),
        ]);
        let roster: &[MailCheck] = v4
            .map(|r| r.mail.as_slice())
            .filter(|m| !m.is_empty())
            .or_else(|| v6.map(|r| r.mail.as_slice()))
            .unwrap_or(&[]);
        for item in roster {
            rows.push(vec![
                item.name.to_owned(),
                mail_cell(v4, item.id, false),
                mail_cell(v6, item.id, true),
            ]);
        }
    }
    rows
}

fn dnsbl_cell(summary: Option<&DnsblSummary>) -> String {
    let Some(summary) = summary else {
        return MISSING.to_owned();
    };
    if summary.checked == 0 && summary.listed.is_empty() {
        return locale::words().unavailable();
    }
    locale::words().ip_dnsbl_cell(summary.checked, summary.refused, &summary.listed)
}

fn port25_cell(checks: Option<&[MailCheck]>) -> String {
    let Some(checks) = checks else {
        return MISSING.to_owned();
    };
    let any_open = checks.iter().any(|c| {
        matches!(
            c.status,
            MailStatus::Deliverable(_) | MailStatus::Rejected(_)
        )
    });
    if any_open {
        return locale::words().ip_port25_cell(true).to_owned();
    }
    let any_blocked = checks.iter().any(|c| c.status == MailStatus::Blocked);
    if any_blocked {
        locale::words().ip_port25_cell(false).to_owned()
    } else {
        MISSING.to_owned()
    }
}

fn mail_cell(rep: Option<&FamilyReport>, id: &str, ipv6: bool) -> String {
    let Some(rep) = rep else {
        return MISSING.to_owned();
    };
    let Some(entry) = rep.mail.iter().find(|m| m.id == id) else {
        return MISSING.to_owned();
    };
    let words = locale::words();
    match &entry.status {
        MailStatus::Deliverable(code) => words.ip_mail_yes(*code),
        MailStatus::Rejected(code) => words.ip_mail_rejected(*code),
        MailStatus::Blocked => words.ip_mail_blocked().to_owned(),
        MailStatus::NoAddress => words.media_cell(MediaCell::NoAddress, ipv6).to_owned(),
        MailStatus::Unexpected(_) => words.media_cell(MediaCell::Error, ipv6).to_owned(),
    }
}

const fn usage_code(usage: UsageType) -> &'static str {
    match usage {
        UsageType::Residential => "residential",
        UsageType::Hosting => "hosting",
        UsageType::Business => "business",
        UsageType::Mobile => "mobile",
        UsageType::Education => "education",
        UsageType::Government => "government",
        UsageType::ContentDelivery => "cdn",
    }
}

const fn level_code(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::VeryHigh => "very_high",
    }
}

const fn greynoise_code(class: GreyNoiseClass) -> &'static str {
    match class {
        GreyNoiseClass::NotObserved => "not_observed",
        GreyNoiseClass::Benign => "benign",
        GreyNoiseClass::Malicious => "malicious",
        GreyNoiseClass::Unknown => "unknown",
    }
}

const fn factor_code(factor: RiskFactor) -> &'static str {
    match factor {
        RiskFactor::Proxy => "proxy",
        RiskFactor::Vpn => "vpn",
        RiskFactor::Tor => "tor",
        RiskFactor::Datacenter => "datacenter",
        RiskFactor::Abuser => "abuser",
        RiskFactor::Robot => "robot",
        RiskFactor::Bogon => "bogon",
    }
}

/// Formats `ip.quality`'s stop reason.
pub(super) fn quality_failure(failure: &quality::Failure) -> String {
    let words = locale::words();
    match failure {
        quality::Failure::NoFamily => words.no_media_family(),
        quality::Failure::NoPublicAddress => words.ip_no_public_address(),
        quality::Failure::InvalidEndpoint(raw) => words.ip_invalid_endpoint(raw),
        quality::Failure::Cancelled => words.interrupted(),
    }
}

/// Renders `ip.media`'s block inside `[IP]`.
pub(super) fn media(out: &mut Out, report: &Report<media::Output, media::Failure>, indent: &str) {
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
            .filter(|service| {
                service.group == group
                    && ((output.ipv4_reachable && service.ipv4.is_some())
                        || (output.ipv6_reachable && service.ipv6.is_some()))
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        out.heading(indent, words.media_group(group.code()));
        let sub = format!("{indent}  ");
        let rows: Vec<Vec<String>> = members
            .iter()
            .map(|service| media_row(service, output.ipv4_reachable, output.ipv6_reachable))
            .collect();
        out.table_left(&sub, &headers, &rows);
        for service in &members {
            for (ipv6, reachable, verdict) in [
                (false, output.ipv4_reachable, &service.ipv4),
                (true, output.ipv6_reachable, &service.ipv6),
            ] {
                if reachable && let Some(Verdict::Unexpected(reason)) = verdict {
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

fn media_row(service: &ServiceResult, ipv4_reachable: bool, ipv6_reachable: bool) -> Vec<String> {
    vec![
        service.name.to_owned(),
        media_cell(
            service.ipv4.as_ref(),
            service.ipv4_unlock,
            ipv4_reachable,
            false,
        ),
        media_cell(
            service.ipv6.as_ref(),
            service.ipv6_unlock,
            ipv6_reachable,
            true,
        ),
    ]
}

fn media_cell(
    verdict: Option<&Verdict>,
    unlock: Option<UnlockMode>,
    reachable: bool,
    ipv6: bool,
) -> String {
    if !reachable {
        return MISSING.to_owned();
    }
    let Some(verdict) = verdict else {
        return MISSING.to_owned();
    };
    let words = locale::words();
    let mode_code = match unlock {
        Some(UnlockMode::Native) => Some("native"),
        Some(UnlockMode::Dns) => Some("dns"),
        None => None,
    };
    match verdict {
        Verdict::Yes { region } => words.media_yes(region.as_deref(), mode_code),
        Verdict::OriginalsOnly { region } => words.media_originals(region.as_deref(), mode_code),
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

/// Formats `ip.media`'s stop reason.
pub(super) fn media_failure(failure: &media::Failure) -> String {
    let words = locale::words();
    match failure {
        media::Failure::InvalidBaseUrl(url) => words.invalid_base_url(url),
        media::Failure::NoService(unknown) => words.no_service(unknown),
        media::Failure::NoFamily => words.no_media_family(),
        media::Failure::Cancelled => words.interrupted(),
    }
}
