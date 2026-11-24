//! Per-provider parser tests across all nine intelligence modules, DNSBL reverse formatting and
//! RFC 5782 open-resolver filtering, and SMTP banner classification.

use std::net::IpAddr;
use std::net::Ipv4Addr;

use bench_api::DnsAnswer;
use bench_api::HttpResponse;
use bench_api::NetError;
use bench_api::SmtpBanner;
use plugin_ip::quality::GreyNoiseClass;
use plugin_ip::quality::MailStatus;
use plugin_ip::quality::RiskFactor;
use plugin_ip::quality::RiskLevel;
use plugin_ip::quality::ScoreVerdict;
use plugin_ip::quality::UsageType;
use plugin_ip::quality::dnsbl;
use plugin_ip::quality::dnsbl::ZoneVerdict;
use plugin_ip::quality::mail;
use plugin_ip::quality::parse::ProviderFailure;
use plugin_ip::quality::providers::dbip;
use plugin_ip::quality::providers::greynoise;
use plugin_ip::quality::providers::ipapi;
use plugin_ip::quality::providers::ipinfo;
use plugin_ip::quality::providers::ipwhois;
use plugin_ip::quality::providers::proxycheck;
use plugin_ip::quality::providers::ripestat;
use plugin_ip::quality::providers::scamalytics;
use plugin_ip::quality::providers::shodan;

fn response(status: u16, headers: &[(&str, &str)], body: &str) -> HttpResponse {
    HttpResponse {
        status,
        final_url: "https://example.invalid/".to_owned(),
        headers: headers
            .iter()
            .map(|(k, v)| ((*k).to_ascii_lowercase(), (*v).to_owned()))
            .collect(),
        body: body.as_bytes().to_vec(),
    }
}

#[test]
fn all_nine_provider_modules_parse_scores_identities_flags_and_waf_gates() {
    // 1. ipapi.is
    let ipapi_res = response(
        200,
        &[],
        concat!(
            r#"{"ip":"198.51.100.7","rir":"APNIC","is_proxy":false,"is_vpn":true,"#,
            r#""is_tor":false,"is_datacenter":true,"is_abuser":false,"is_crawler":false,"#,
            r#""is_bogon":false,"location":{"country_code":"SG"},"#,
            r#""company":{"name":"Example Cloud","type":"hosting","abuser_score":"0.0039 (Low)"},"#,
            r#""asn":{"asn":13335,"org":"Cloudflare, Inc.","country":"US","type":"hosting"}}"#
        ),
    );
    let ipapi_data = ipapi::parse(&ipapi_res).expect("ipapi parse");
    assert_eq!(ipapi_data.asn, Some(13335));
    assert_eq!(ipapi_data.rir.as_deref(), Some("APNIC"));
    assert_eq!(ipapi_data.registered_country.as_deref(), Some("US"));
    assert_eq!(ipapi_data.geo_country.as_deref(), Some("SG"));
    assert_eq!(ipapi_data.usage, Some(UsageType::Hosting));
    assert_eq!(
        ipapi_data.score,
        Some(ScoreVerdict::Ratio {
            text: "0.0039".to_owned(),
            level: RiskLevel::Low,
        })
    );
    assert!(ipapi_data.flags.contains(&(RiskFactor::Vpn, true)));
    assert!(ipapi_data.flags.contains(&(RiskFactor::Datacenter, true)));

    // 2. Scamalytics (normal HTML + Cloudflare Turnstile gate)
    let scam_html = response(
        200,
        &[],
        &format!(
            concat!(
                r#"<pre>{{"ip":"198.51.100.7","score":"12","risk":"low"}}</pre>"#,
                "<p>Dedicated Server</p>",
                "<tr><th>Anonymizing VPN</th><td><div class=\"risk no\">No</div></td></tr>",
                "<tr><th>Server</th><td><div class=\"risk yes\">Yes</div></td></tr>",
                "<th>Tor Exit Node</th>{pad}"
            ),
            pad = "\u{2014}".repeat(100)
        ),
    );
    let scam_data = scamalytics::parse(&scam_html).expect("scamalytics parse");
    assert_eq!(
        scam_data.score,
        Some(ScoreVerdict::Numeric {
            score: 12,
            max: 100,
            level: RiskLevel::Low,
        })
    );
    assert!(scam_data.flags.contains(&(RiskFactor::Vpn, false)));
    assert!(scam_data.flags.contains(&(RiskFactor::Datacenter, true)));

    let scam_waf = response(403, &[("cf-mitigated", "challenge")], "");
    assert_eq!(
        scamalytics::parse(&scam_waf),
        Err(ProviderFailure::BlockedWaf)
    );

    // 3. ProxyCheck
    let pc_res = response(
        200,
        &[],
        concat!(
            r#"{"status":"ok","198.51.100.7":{"asn":"AS13335","#,
            r#""provider":"","organisation":"Cloudflare","#,
            r#""isocode":"SG","proxy":"no","vpn":"no","type":"Hosting","risk":15}}"#
        ),
    );
    let pc_data = proxycheck::parse(&pc_res).expect("proxycheck parse");
    assert_eq!(pc_data.asn, Some(13335));
    assert_eq!(pc_data.org.as_deref(), Some("Cloudflare"));
    assert_eq!(pc_data.geo_country.as_deref(), Some("SG"));
    assert_eq!(pc_data.usage, Some(UsageType::Hosting));
    assert!(pc_data.flags.contains(&(RiskFactor::Vpn, false)));
    assert_eq!(
        pc_data.score,
        Some(ScoreVerdict::Numeric {
            score: 15,
            max: 100,
            level: RiskLevel::Low,
        })
    );

    // 4. RIPEstat
    let ripe_res = response(
        200,
        &[],
        concat!(
            r#"{"data":{"authorities":["arin"],"records":[[{"key":"Country","value":"US"},"#,
            r#"{"key":"OrgName","value":"Cloudflare, Inc."}]]}}"#
        ),
    );
    let ripe_data = ripestat::parse(&ripe_res).expect("ripestat parse");
    assert_eq!(ripe_data.rir.as_deref(), Some("ARIN"));
    assert_eq!(ripe_data.registered_country.as_deref(), Some("US"));
    assert_eq!(ripe_data.org.as_deref(), Some("Cloudflare, Inc."));

    // 5. GreyNoise (404 clean + 200 malicious)
    let gn_404 = response(404, &[], r#"{"message":"IP not observed"}"#);
    let gn_clean = greynoise::parse(&gn_404).expect("greynoise 404");
    assert_eq!(
        gn_clean.score,
        Some(ScoreVerdict::GreyNoise {
            riot: false,
            noise: false,
            classification: GreyNoiseClass::NotObserved,
        })
    );
    let gn_bad = response(
        200,
        &[],
        r#"{"noise":true,"riot":false,"classification":"malicious"}"#,
    );
    let gn_mal = greynoise::parse(&gn_bad).expect("greynoise 200");
    assert!(gn_mal.flags.contains(&(RiskFactor::Abuser, true)));

    // 6. Shodan InternetDB
    let shodan_res = response(
        200,
        &[],
        concat!(
            r#"{"ip":"198.51.100.7","ports":[22,443],"tags":["cloud"],"#,
            r#""vulns":[],"hostnames":["h.example"]}"#
        ),
    );
    let shodan_data = shodan::parse(&shodan_res).expect("shodan parse");
    assert_eq!(
        shodan_data.score,
        Some(ScoreVerdict::Shodan {
            ports: 2,
            vulns: 0,
            flagged: false,
        })
    );
    assert_eq!(shodan_data.hostname.as_deref(), Some("h.example"));
    assert!(shodan_data.flags.contains(&(RiskFactor::Datacenter, true)));

    // 7. IPinfo
    let info_res = response(
        200,
        &[],
        concat!(
            r#"{"ip":"198.51.100.7","hostname":"ptr.example","#,
            r#""country":"SG","org":"AS13335 Cloudflare, Inc."}"#
        ),
    );
    let info_data = ipinfo::parse(&info_res).expect("ipinfo parse");
    assert_eq!(info_data.asn, Some(13335));
    assert_eq!(info_data.org.as_deref(), Some("Cloudflare, Inc."));
    assert_eq!(info_data.geo_country.as_deref(), Some("SG"));
    assert_eq!(info_data.hostname.as_deref(), Some("ptr.example"));

    // 8. DB-IP
    let dbip_res = response(200, &[], r#"{"countryCode":"SG"}"#);
    assert_eq!(
        dbip::parse(&dbip_res).expect("dbip").geo_country.as_deref(),
        Some("SG")
    );
    assert_eq!(
        dbip::parse(&response(200, &[], r#"{"countryCode":"ZZ"}"#))
            .expect("dbip placeholder")
            .geo_country,
        None
    );
    assert!(greynoise::parse(&response(404, &[], "<html>404</html>")).is_err());
    assert!(greynoise::parse(&response(200, &[], "{}")).is_err());
    assert!(shodan::parse(&response(200, &[], "{}")).is_err());
    assert_eq!(
        shodan::parse(&response(200, &[], r#"{"detail":"Rate limit exceeded"}"#)),
        Err(plugin_ip::quality::parse::ProviderFailure::RateLimited)
    );
    let num_org = ipinfo::parse(&response(200, &[], r#"{"org":"1984 ehf"}"#)).expect("ipinfo");
    assert_eq!(num_org.asn, None);
    assert_eq!(num_org.org.as_deref(), Some("1984 ehf"));
    let bad_ratio = ipapi::parse(&response(
        200,
        &[],
        r#"{"company":{"abuser_score":"NaN"},"asn":{"abuser_score":"0.0100 (Elevated)"}}"#,
    ))
    .expect("ipapi fallback");
    assert_eq!(
        bad_ratio.score,
        Some(ScoreVerdict::Ratio {
            text: "0.0100".to_owned(),
            level: RiskLevel::Medium,
        })
    );

    // 9. IPWHOIS
    let whois_res = response(
        200,
        &[],
        concat!(
            r#"{"success":true,"country_code":"SG","#,
            r#""connection":{"asn":13335,"org":"","isp":"Cloudflare"}}"#
        ),
    );
    let whois_data = ipwhois::parse(&whois_res).expect("ipwhois");
    assert_eq!(whois_data.geo_country.as_deref(), Some("SG"));
    assert_eq!(whois_data.asn, Some(13335));
    assert_eq!(whois_data.org.as_deref(), Some("Cloudflare"));
}

#[test]
fn dnsbl_query_names_and_rfc5782_policy_filtering_prevent_false_positives() {
    let v4: IpAddr = "198.51.100.7".parse().expect("v4");
    assert_eq!(
        dnsbl::query_name(v4, "zen.spamhaus.org"),
        "7.100.51.198.zen.spamhaus.org"
    );

    let v6: IpAddr = "2001:db8::2a".parse().expect("v6");
    assert_eq!(
        dnsbl::query_name(v6, "dnsbl.dronebl.org"),
        "a.2.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.b.d.0.1.0.0.2.dnsbl.dronebl.org"
    );

    assert_eq!(
        dnsbl::classify(&Ok(DnsAnswer::NotFound)),
        ZoneVerdict::Clean
    );
    assert_eq!(
        dnsbl::classify(&Ok(DnsAnswer::Addresses(vec![Ipv4Addr::new(127, 0, 0, 2)]))),
        ZoneVerdict::Listed
    );
    // Open-resolver refusal codes in 127.255.255.0/24 must never count as blacklist hits.
    assert_eq!(
        dnsbl::classify(&Ok(DnsAnswer::Addresses(vec![Ipv4Addr::new(
            127, 255, 255, 254
        )]))),
        ZoneVerdict::Refused
    );
    // Captive/ISP hijacked non-127.0.0.0/8 records must never count as blacklist hits.
    assert_eq!(
        dnsbl::classify(&Ok(DnsAnswer::Addresses(vec![Ipv4Addr::new(
            198, 51, 100, 1
        )]))),
        ZoneVerdict::Refused
    );
}

#[test]
fn mail_classifier_distinguishes_220_rejection_codes_blocks_and_missing_aaaa() {
    assert_eq!(
        mail::classify(&Ok(SmtpBanner { code: 220 })),
        MailStatus::Deliverable(220)
    );
    assert_eq!(
        mail::classify(&Ok(SmtpBanner { code: 554 })),
        MailStatus::Rejected(554)
    );
    assert_eq!(
        mail::classify(&Err(NetError::TimedOut)),
        MailStatus::Blocked
    );
    assert_eq!(
        mail::classify(&Err(NetError::NoAddress)),
        MailStatus::NoAddress
    );
}
