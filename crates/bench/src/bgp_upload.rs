//! Anonymous unlisted SVG upload to `imageupload.app` for `network.bgp` in `text` reports.
//!
//! Rendered SVGs contain only Autonomous System numbers, carrier names, peer weights, and prepend
//! badges — never the host's public IP address or prefix. Upload failures record a non-fatal note
//! on `bgp::Output` so `network.bgp` still completes cleanly when offline or rate-limited.

use std::time::Duration;

use bench_api::CancellationToken;
use futures_util::StreamExt;
use plugin_network::bgp::FamilyBgp;
use plugin_network::bgp::Output;
use reqwest::Client;
use reqwest::header;
use serde::Deserialize;

use crate::fetcher;
use crate::locale;
use crate::orchestrator::Run;
use crate::report;
use crate::results::Measured;

/// Default anonymous upload endpoint on `imageupload.app`.
pub const DEFAULT_UPLOAD_URL: &str = "https://imageupload.app/api/upload";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MULTIPART_BOUNDARY: &str = "----lowendaffBenchBgpSvg7f3a9c2e4b1d";

#[derive(Deserialize)]
struct UploadEnvelope {
    success: Option<bool>,
    #[serde(rename = "imageKey")]
    image_key: Option<String>,
    #[serde(default)]
    files: Vec<UploadedFile>,
    error: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct UploadedFile {
    path: Option<String>,
}

/// Renders and uploads topology SVGs for every completed `network.bgp` run in `runs` whose
/// `settings.upload` is `true`. Skips external network traffic when `custom_endpoints` is set
/// without an explicit `upload_url`.
pub async fn populate_runs(runs: &mut [Run<Measured>], cancel: &CancellationToken) {
    if cancel.is_cancelled() {
        return;
    }

    let mut targets: Vec<&mut Output> = Vec::new();
    for run in runs {
        let Some(Measured::NetworkBgp(report)) = &mut run.result else {
            continue;
        };
        let Some(output) = &mut report.output else {
            continue;
        };
        if !output.settings.upload {
            continue;
        }
        if output.settings.custom_endpoints && output.settings.upload_url.is_none() {
            continue;
        }
        let has_v4 = output
            .ipv4
            .as_deref()
            .is_some_and(|rep| rep.announced && !rep.paths.is_empty() && rep.diagram_url.is_none());
        let has_v6 = output
            .ipv6
            .as_deref()
            .is_some_and(|rep| rep.announced && !rep.paths.is_empty() && rep.diagram_url.is_none());
        if has_v4 || has_v6 {
            targets.push(output);
        }
    }

    if targets.is_empty() {
        return;
    }

    let client = match build_client() {
        Ok(client) => client,
        Err(reason) => {
            for output in targets {
                record_client_error(output, &reason);
            }
            return;
        }
    };

    for output in targets {
        if cancel.is_cancelled() {
            return;
        }
        populate_output(&client, output, cancel).await;
    }
}

fn build_client() -> Result<Client, String> {
    fetcher::builder()?
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(UPLOAD_TIMEOUT)
        .build()
        .map_err(|err| locale::words().http_client_failed(&err))
}

fn record_client_error(output: &mut Output, reason: &str) {
    for (ipv6, maybe_rep) in [
        (false, output.ipv4.as_deref()),
        (true, output.ipv6.as_deref()),
    ] {
        if maybe_rep.is_some_and(|rep| rep.announced && !rep.paths.is_empty()) {
            output.upload_notes.push((ipv6, reason.to_owned()));
        }
    }
}

async fn populate_output(client: &Client, output: &mut Output, cancel: &CancellationToken) {
    let endpoint = output
        .settings
        .upload_url
        .as_deref()
        .unwrap_or(DEFAULT_UPLOAD_URL)
        .to_owned();

    let v4_svg = render_family_svg(output.ipv4.as_deref());
    let v6_svg = render_family_svg(output.ipv6.as_deref());

    let v4_fut = async {
        match v4_svg {
            Some(svg) => Some(upload_svg(client, &endpoint, "bgp-ipv4.svg", &svg, cancel).await),
            None => None,
        }
    };
    let v6_fut = async {
        match v6_svg {
            Some(svg) => Some(upload_svg(client, &endpoint, "bgp-ipv6.svg", &svg, cancel).await),
            None => None,
        }
    };

    let (v4_res, v6_res) = tokio::join!(v4_fut, v6_fut);
    if cancel.is_cancelled() {
        return;
    }

    apply_upload_result(&mut output.ipv4, &mut output.upload_notes, false, v4_res);
    apply_upload_result(&mut output.ipv6, &mut output.upload_notes, true, v6_res);
}

fn render_family_svg(rep: Option<&FamilyBgp>) -> Option<String> {
    let rep = rep?;
    if !rep.announced || rep.paths.is_empty() || rep.diagram_url.is_some() {
        return None;
    }
    let graph = report::bgp_graph::layout(rep, None)?;
    Some(report::bgp_graph::render_svg(&graph))
}

fn apply_upload_result(
    slot: &mut Option<Box<FamilyBgp>>,
    notes: &mut Vec<(bool, String)>,
    ipv6: bool,
    result: Option<Result<String, String>>,
) {
    let Some(outcome) = result else {
        return;
    };
    match outcome {
        Ok(url) => {
            if let Some(rep) = slot.as_deref_mut() {
                rep.diagram_url = Some(url);
            }
        }
        Err(reason) => {
            if !reason.is_empty() {
                notes.push((ipv6, reason));
            }
        }
    }
}

async fn upload_svg(
    client: &Client,
    endpoint: &str,
    filename: &str,
    svg: &str,
    cancel: &CancellationToken,
) -> Result<String, String> {
    tracing::debug!("network.bgp uploading {filename} to {endpoint}");
    let body = build_multipart(MULTIPART_BOUNDARY, filename, svg);
    let content_type = format!("multipart/form-data; boundary={MULTIPART_BOUNDARY}");

    let send_fut = async {
        let response = client
            .post(endpoint)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::ACCEPT, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|err| locale::words().request_failed(&fetcher::causes(&err), ""))?;

        let status = response.status().as_u16();
        let mut bytes = Vec::with_capacity(512);
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|err| locale::words().request_failed(&fetcher::causes(&err), ""))?;
            let remaining = MAX_RESPONSE_BYTES.saturating_sub(bytes.len());
            if chunk.len() > remaining {
                bytes.extend_from_slice(&chunk[..remaining]);
                break;
            }
            bytes.extend_from_slice(&chunk);
        }
        parse_response(status, &bytes, origin_of(endpoint))
    };

    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(String::new()),
        res = send_fut => res,
    }
}

/// Builds an RFC 7578 `multipart/form-data` payload for `POST /api/upload` with unlisted
/// visibility and unlimited retention.
#[must_use]
pub fn build_multipart(boundary: &str, filename: &str, svg: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(svg.len() + 512);
    buf.extend_from_slice(b"--");
    buf.extend_from_slice(boundary.as_bytes());
    buf.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"images\"; filename=\"");
    buf.extend_from_slice(filename.as_bytes());
    buf.extend_from_slice(b"\"\r\nContent-Type: image/svg+xml\r\n\r\n");
    buf.extend_from_slice(svg.as_bytes());
    buf.extend_from_slice(b"\r\n");

    for (name, value) in [
        ("isPublic", "false"),
        ("visibility", "unlisted"),
        ("storageDuration", "unlimited"),
    ] {
        buf.extend_from_slice(b"--");
        buf.extend_from_slice(boundary.as_bytes());
        buf.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b"\"\r\n\r\n");
        buf.extend_from_slice(value.as_bytes());
        buf.extend_from_slice(b"\r\n");
    }

    buf.extend_from_slice(b"--");
    buf.extend_from_slice(boundary.as_bytes());
    buf.extend_from_slice(b"--\r\n");
    buf
}

/// Extracts `scheme://host[:port]` from `endpoint`.
#[must_use]
pub fn origin_of(endpoint: &str) -> &str {
    let trimmed = endpoint.trim();
    let Some(after_scheme) = trimmed.find("://").map(|idx| idx + 3) else {
        return trimmed.trim_end_matches('/');
    };
    match trimmed[after_scheme..].find('/') {
        Some(rel_slash) => &trimmed[..after_scheme + rel_slash],
        None => trimmed,
    }
}

/// Parses an `imageupload.app` JSON response into a validated `{origin}/i/{imageKey}` link.
pub fn parse_response(status: u16, body: &[u8], origin: &str) -> Result<String, String> {
    let parsed: Option<UploadEnvelope> = serde_json::from_slice(body).ok();
    if !(200..300).contains(&status) {
        if let Some(env) = parsed
            && let Some(msg) = env
                .error
                .as_deref()
                .or(env.message.as_deref())
                .map(sanitize_reason)
                .filter(|s| !s.is_empty())
        {
            return Err(format!("HTTP {status}: {msg}"));
        }
        return Err(locale::words().http_status(status));
    }

    let Some(env) = parsed else {
        return Err("invalid JSON response".to_owned());
    };
    if env.success == Some(false) {
        let why = env
            .error
            .as_deref()
            .or(env.message.as_deref())
            .map(sanitize_reason)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "upload rejected".to_owned());
        return Err(why);
    }

    let base = origin.trim_end_matches('/');
    if let Some(key) = env.image_key.as_deref().map(str::trim)
        && is_safe_key(key)
    {
        return Ok(format!("{base}/i/{key}"));
    }
    if let Some(path) = env
        .files
        .first()
        .and_then(|file| file.path.as_deref())
        .map(str::trim)
        && let Some(key) = path.strip_prefix("/i/")
        && is_safe_key(key)
    {
        return Ok(format!("{base}/i/{key}"));
    }

    Err("response missing valid imageKey".to_owned())
}

fn is_safe_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn sanitize_reason(raw: &str) -> String {
    raw.chars()
        .filter(|ch| !ch.is_control())
        .take(120)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use bench_api::CancellationToken;
    use plugin_network::bgp;
    use plugin_network::bgp::BgpPath;
    use plugin_network::bgp::FamilyBgp;
    use plugin_network::bgp::OriginSummary;
    use plugin_network::bgp::RpkiStatus;
    use plugin_network::bgp::UpstreamShare;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    use crate::orchestrator::Run;
    use crate::orchestrator::Status;
    use crate::results::Measured;
    use crate::results::Report;

    fn sample_bgp_run(settings: bgp::Settings) -> Run<Measured> {
        let mut asn_names = BTreeMap::new();
        asn_names.insert(64500, "Origin Cloud".to_owned());
        asn_names.insert(3356, "Lumen".to_owned());
        let v4 = FamilyBgp {
            address: "203.0.113.7".parse().expect("ipv4"),
            announced: true,
            prefix: Some("203.0.113.0/24".to_owned()),
            prefix_base: Some("203.0.113.0".parse().expect("base")),
            prefix_len: Some(24),
            origin_asn: Some(64500),
            origins: vec![OriginSummary {
                asn: 64500,
                peers: 10,
            }],
            rpki: RpkiStatus::Valid,
            total_peers: 10,
            distinct_paths: 1,
            upstreams: vec![UpstreamShare {
                asn: 3356,
                peers: 10,
            }],
            paths: vec![BgpPath {
                asns: vec![3356, 64500],
                asns_raw: vec![3356, 64500],
                peers: 10,
                is_best: true,
            }],
            asn_names,
            diagram_url: None,
        };
        Run {
            id: "network.bgp",
            group: "network",
            title: Some("BGP paths"),
            started: Some(Duration::ZERO),
            elapsed: Some(Duration::from_millis(500)),
            status: Status::Completed,
            result: Some(Measured::NetworkBgp(Report {
                output: Some(bgp::Output {
                    settings,
                    ipv4_reachable: true,
                    ipv6_reachable: false,
                    ipv4: Some(Box::new(v4)),
                    ipv6: None,
                    notes: Vec::new(),
                    upload_notes: Vec::new(),
                }),
                stop: None,
            })),
        }
    }

    #[test]
    fn multipart_payload_sets_unlisted_unlimited_svg_fields() {
        let bytes = super::build_multipart("bnd123", "bgp-ipv4.svg", "<svg/>");
        let text = String::from_utf8(bytes).expect("UTF-8 multipart");
        assert!(
            text.contains(
                "--bnd123\r\nContent-Disposition: form-data; name=\"images\"; \
                 filename=\"bgp-ipv4.svg\"\r\nContent-Type: image/svg+xml\r\n\r\n<svg/>\r\n"
            ),
            "{text}"
        );
        assert!(text.contains("name=\"isPublic\"\r\n\r\nfalse\r\n"));
        assert!(text.contains("name=\"visibility\"\r\n\r\nunlisted\r\n"));
        assert!(text.contains("name=\"storageDuration\"\r\n\r\nunlimited\r\n"));
        assert!(text.ends_with("--bnd123--\r\n"));
    }

    #[test]
    fn response_parser_validates_keys_and_surfaces_http_errors() {
        assert_eq!(
            super::origin_of("https://imageupload.app/api/upload"),
            "https://imageupload.app"
        );
        assert_eq!(
            super::origin_of("http://127.0.0.1:18083/api/upload"),
            "http://127.0.0.1:18083"
        );

        let ok_body = br#"{"success":true,"imageKey":"9d0e951d4f615260d67a"}"#;
        assert_eq!(
            super::parse_response(200, ok_body, "https://imageupload.app"),
            Ok("https://imageupload.app/i/9d0e951d4f615260d67a".to_owned())
        );

        let fallback_body = br#"{"success":true,"files":[{"path":"/i/abc_123-XYZ"}]}"#;
        assert_eq!(
            super::parse_response(200, fallback_body, "https://imageupload.app"),
            Ok("https://imageupload.app/i/abc_123-XYZ".to_owned())
        );

        let unsafe_key = br#"{"success":true,"imageKey":"../evil\nkey"}"#;
        assert!(super::parse_response(200, unsafe_key, "https://imageupload.app").is_err());

        let rate_limited = br#"{"success":false,"error":"Too many uploads"}"#;
        assert_eq!(
            super::parse_response(429, rate_limited, "https://imageupload.app"),
            Err("HTTP 429: Too many uploads".to_owned())
        );
    }

    #[tokio::test]
    async fn populate_runs_uploads_anonymized_svg_and_records_non_fatal_note_on_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            // Request 1: 200 OK after verifying SVG + no leaked IP.
            if let Ok((mut stream, _)) = listener.accept().await {
                let req = read_http_request(&mut stream).await;
                assert!(req.contains("Content-Type: image/svg+xml"));
                assert!(req.contains("AS64500"));
                assert!(!req.contains("203.0.113."));
                let body = br#"{"success":true,"imageKey":"testkey0123456789ab"}"#;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&[head.as_bytes(), body].concat()).await;
            }
            // Request 2: 503 Service Unavailable.
            if let Ok((mut stream, _)) = listener.accept().await {
                let _ = read_http_request(&mut stream).await;
                let body = br#"{"success":false,"error":"maintenance"}"#;
                let head = format!(
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&[head.as_bytes(), body].concat()).await;
            }
        });

        let endpoint = format!("http://{addr}/api/upload");

        // 1. Successful upload populates `diagram_url`.
        let mut runs = vec![sample_bgp_run(bgp::Settings {
            custom_endpoints: true,
            upload: true,
            upload_url: Some(endpoint.clone()),
        })];
        super::populate_runs(&mut runs, &CancellationToken::new()).await;
        let Some(Measured::NetworkBgp(rep)) = &runs[0].result else {
            panic!("expected bgp");
        };
        let out = rep.output.as_ref().expect("output");
        assert_eq!(
            out.ipv4.as_ref().unwrap().diagram_url.as_deref(),
            Some(format!("http://{addr}/i/testkey0123456789ab").as_str())
        );
        assert!(out.upload_notes.is_empty());

        // 2. HTTP 503 records `upload_notes` while keeping `Status::Completed`.
        let mut runs_err = vec![sample_bgp_run(bgp::Settings {
            custom_endpoints: true,
            upload: true,
            upload_url: Some(endpoint),
        })];
        super::populate_runs(&mut runs_err, &CancellationToken::new()).await;
        assert_eq!(runs_err[0].status, Status::Completed);
        let Some(Measured::NetworkBgp(rep_err)) = &runs_err[0].result else {
            panic!("expected bgp");
        };
        let out_err = rep_err.output.as_ref().expect("output");
        assert!(out_err.ipv4.as_ref().unwrap().diagram_url.is_none());
        assert_eq!(
            out_err.upload_notes,
            vec![(false, "HTTP 503: maintenance".to_owned())]
        );

        // 3. `upload: false` and `custom_endpoints` without `upload_url` make zero connections.
        let mut disabled = vec![
            sample_bgp_run(bgp::Settings {
                custom_endpoints: false,
                upload: false,
                upload_url: Some("http://127.0.0.1:9/api/upload".to_owned()),
            }),
            sample_bgp_run(bgp::Settings {
                custom_endpoints: true,
                upload: true,
                upload_url: None,
            }),
        ];
        super::populate_runs(&mut disabled, &CancellationToken::new()).await;
        for run in &disabled {
            let Some(Measured::NetworkBgp(r)) = &run.result else {
                panic!("expected bgp");
            };
            let o = r.output.as_ref().unwrap();
            assert!(o.ipv4.as_ref().unwrap().diagram_url.is_none());
            assert!(o.upload_notes.is_empty());
        }
    }

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
        let mut buf = Vec::with_capacity(4096);
        let mut chunk = [0_u8; 2048];
        let mut expected_total = None;
        loop {
            let n = stream.read(&mut chunk).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            if expected_total.is_none()
                && let Some(hdr_end) = buf.windows(4).position(|w| w == b"\r\n\r\n")
            {
                let body_start = hdr_end + 4;
                let hdrs = String::from_utf8_lossy(&buf[..hdr_end]).to_ascii_lowercase();
                let len = hdrs
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                expected_total = Some(body_start + len);
            }
            if let Some(total) = expected_total
                && buf.len() >= total
            {
                break;
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    }
}
