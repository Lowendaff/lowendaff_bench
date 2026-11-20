//! A plugin's context: what the plugin publishes and reports reaches the orchestrator's end,
//! the last publish wins, and a plugin never fails because nobody listens any more.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Channels;
use bench_api::Context;
use bench_api::DnsAnswer;
use bench_api::HttpClient;
use bench_api::HttpError;
use bench_api::HttpFuture;
use bench_api::HttpMethod;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use bench_api::Measurement;
use bench_api::NetClient;
use bench_api::NetError;
use bench_api::NetFuture;
use bench_api::Progress;
use bench_api::RedirectPolicy;
use bench_api::SmtpBanner;
use bench_api::Step;
use bench_api::ToolEnd;
use bench_api::ToolRun;
use bench_api::ToolVersion;
use bench_host::profile::HostProfile;
use serde::Serialize;
use tokio::time::Instant;

#[derive(Debug, Clone, PartialEq, Serialize)]
struct Output {
    runs: u32,
}

impl Measurement for Output {
    fn tools(&self) -> Vec<ToolVersion> {
        vec![ToolVersion {
            name: "probe",
            version: "1.0".to_owned(),
        }]
    }
}

fn context() -> (Context<Output>, Channels<Output>, CancellationToken) {
    let tools = BTreeMap::from([("probe".to_owned(), PathBuf::from("/run/tools/probe"))]);
    let files = BTreeMap::from([
        (
            "list".to_owned(),
            Ok(PathBuf::from("/run/files/probe/list")),
        ),
        (
            "cache".to_owned(),
            Err("download failed: HTTP 404".to_owned()),
        ),
    ]);
    let host = HostProfile {
        arch: "riscv64".to_owned(),
        ..HostProfile::default()
    };
    let cancel = CancellationToken::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    let (ctx, channels) = Context::new(
        Arc::new(host),
        Arc::new(tools),
        Arc::new(files),
        deadline,
        PathBuf::from("/run/work"),
        cancel.clone(),
    );
    (ctx, channels, cancel)
}

#[test]
fn the_last_publish_wins_and_nothing_is_published_at_first() {
    let (ctx, channels, _) = context();
    assert_eq!(*channels.published.borrow(), None);
    ctx.publish(&Output { runs: 1 });
    ctx.publish(&Output { runs: 2 });
    assert_eq!(*channels.published.borrow(), Some(Output { runs: 2 }));
}

#[test]
fn steps_and_tool_runs_arrive_in_order() {
    let (ctx, mut channels, _) = context();
    let run = ToolRun {
        command: vec!["probe".to_owned(), "--fast".to_owned()],
        end: ToolEnd::Exit(1),
        tail: vec!["no disk".to_owned()],
    };
    ctx.progress(Step::Test { threads: 1 });
    ctx.ran(run.clone());
    ctx.progress(Step::Test { threads: 4 });
    let step = |threads| Ok(Progress::Step(Step::Test { threads }));
    assert_eq!(channels.progress.try_recv(), step(1));
    assert_eq!(channels.progress.try_recv(), Ok(Progress::Tool(run)));
    assert_eq!(channels.progress.try_recv(), step(4));
    assert!(
        channels.progress.try_recv().is_err(),
        "nothing else was sent"
    );
}

#[test]
fn a_plugin_keeps_running_when_nobody_listens() {
    let (ctx, channels, _) = context();
    drop(channels);
    ctx.progress(Step::TestFile);
    ctx.ran(ToolRun {
        command: vec!["probe".to_owned()],
        end: ToolEnd::Cancelled,
        tail: Vec::new(),
    });
    ctx.publish(&Output { runs: 3 });
}

#[test]
fn the_context_hands_out_host_tools_files_why_files_are_missing_directory_deadline_and_token() {
    let (ctx, _channels, cancel) = context();
    assert_eq!(ctx.host().arch, "riscv64");
    assert_eq!(
        ctx.tool("probe"),
        Some(PathBuf::from("/run/tools/probe").as_path())
    );
    assert_eq!(ctx.tool("undeclared"), None);
    assert_eq!(
        ctx.file("list"),
        Some(PathBuf::from("/run/files/probe/list").as_path())
    );
    assert_eq!(ctx.file("probe"), None, "a tool is no file");
    assert_eq!(ctx.missing("list"), None, "a file that came");
    assert_eq!(
        ctx.file("cache"),
        None,
        "an optional file that did not come"
    );
    assert_eq!(ctx.missing("cache"), Some("download failed: HTTP 404"));
    assert_eq!(ctx.missing("undeclared"), None);
    assert_eq!(ctx.work_dir(), PathBuf::from("/run/work").as_path());
    assert!(ctx.deadline() > Instant::now());
    assert!(!ctx.cancel_token().is_cancelled());
    cancel.cancel();
    assert!(
        ctx.cancel_token().is_cancelled(),
        "the plugin sees the orchestrator's cancel"
    );
    assert_eq!(Output { runs: 0 }.tools()[0].version, "1.0");
}

struct StubHttp;

impl HttpClient for StubHttp {
    fn send<'a>(&'a self, request: &'a HttpRequest) -> HttpFuture<'a> {
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                final_url: request.url.clone(),
                headers: vec![
                    ("content-type".to_owned(), "application/json".to_owned()),
                    ("set-cookie".to_owned(), "a=1".to_owned()),
                    ("set-cookie".to_owned(), "b=2".to_owned()),
                ],
                body: b"{\"ok\":true}".to_vec(),
            })
        })
    }
}

struct PendingHttp;

impl HttpClient for PendingHttp {
    fn send<'a>(&'a self, _request: &'a HttpRequest) -> HttpFuture<'a> {
        Box::pin(async { std::future::pending().await })
    }
}

#[tokio::test]
async fn context_probe_uses_the_configured_http_client_and_stops_on_cancellation() {
    let (default_ctx, _channels, _) = context();
    let get = HttpRequest::get(IpFamily::Ipv4, "https://example.com")
        .with_header("accept", "application/json")
        .with_redirect(RedirectPolicy::Follow { max: 3 })
        .with_timeout(Duration::from_secs(5))
        .with_max_bytes(1024);
    assert_eq!(get.method, HttpMethod::Get);
    assert_eq!(get.redirect, RedirectPolicy::Follow { max: 3 });
    assert_eq!(get.max_bytes, 1024);
    assert!(matches!(
        default_ctx.probe(&get).await,
        Err(HttpError::Transport(_))
    ));

    let post = HttpRequest::post(IpFamily::Ipv6, "https://example.com/api", b"{}".to_vec());
    assert_eq!(post.method, HttpMethod::Post);
    assert_eq!(post.body.as_deref(), Some(b"{}".as_slice()));

    let (ctx, _channels, _) = context();
    let ctx = ctx.with_http(Arc::new(StubHttp));
    let response = ctx.probe(&get).await.expect("stub response");
    assert_eq!(response.status, 200);
    assert_eq!(response.header("Content-Type"), Some("application/json"));
    assert_eq!(response.header_values("Set-Cookie"), vec!["a=1", "b=2"]);
    assert_eq!(response.text(), "{\"ok\":true}");

    let (ctx, _channels, cancel) = context();
    let ctx = ctx.with_http(Arc::new(PendingHttp));
    cancel.cancel();
    assert_eq!(ctx.probe(&get).await, Err(HttpError::Cancelled));
}

struct StubNet;

impl NetClient for StubNet {
    fn dns_a<'a>(
        &'a self,
        qname: &'a str,
        _server: Option<std::net::SocketAddr>,
        _timeout: Duration,
    ) -> NetFuture<'a, DnsAnswer> {
        Box::pin(async move {
            if qname.starts_with("2.0.0.127.") {
                Ok(DnsAnswer::Addresses(vec![std::net::Ipv4Addr::new(
                    127, 0, 0, 2,
                )]))
            } else {
                Ok(DnsAnswer::NotFound)
            }
        })
    }

    fn smtp_banner<'a>(
        &'a self,
        _family: IpFamily,
        _host: &'a str,
        _port: u16,
        _timeout: Duration,
    ) -> NetFuture<'a, SmtpBanner> {
        Box::pin(async { Ok(SmtpBanner { code: 220 }) })
    }
}

#[tokio::test]
async fn context_net_uses_the_configured_client_and_stops_on_cancellation() {
    let (default_ctx, _channels, _) = context();
    let limit = Duration::from_secs(3);
    assert!(matches!(
        default_ctx
            .dns_a("1.0.0.127.example.invalid", None, limit)
            .await,
        Err(NetError::Transport(_))
    ));
    assert!(matches!(
        default_ctx
            .smtp_banner(IpFamily::Ipv4, "mx.example.invalid", 25, limit)
            .await,
        Err(NetError::Transport(_))
    ));

    let (ctx, _channels, cancel) = context();
    let ctx = ctx.with_net(Arc::new(StubNet));
    assert_eq!(
        ctx.dns_a("2.0.0.127.zen.example", None, limit).await,
        Ok(DnsAnswer::Addresses(vec![std::net::Ipv4Addr::new(
            127, 0, 0, 2
        )]))
    );
    assert_eq!(
        ctx.dns_a("1.0.0.127.zen.example", None, limit).await,
        Ok(DnsAnswer::NotFound)
    );
    assert_eq!(
        ctx.smtp_banner(IpFamily::Ipv4, "mx.example", 25, limit)
            .await,
        Ok(SmtpBanner { code: 220 })
    );

    cancel.cancel();
    assert_eq!(
        ctx.dns_a("2.0.0.127.zen.example", None, limit).await,
        Err(NetError::Cancelled)
    );
    assert_eq!(
        ctx.smtp_banner(IpFamily::Ipv4, "mx.example", 25, limit)
            .await,
        Err(NetError::Cancelled)
    );
}
