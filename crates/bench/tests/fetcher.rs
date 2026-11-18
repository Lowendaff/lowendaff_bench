//! The download adapter against a local plain-HTTP server: streaming, byte ranges, HTTP errors,
//! connection errors and the proxy hint they end in, and provisioning through the real client,
//! whole or in ranges, and again where a download broke off.
//! Building the client also proves the preconfigured rustls config matches reqwest's rustls
//! version. The tests expect no proxy variables, as on CI's runners.

use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use bench::fetcher;
use bench::fetcher::Fetcher;
use bench::provision;
use bench::provision::DownloadError;
use bench::provision::MIN_RANGE;
use bench::provision::Part;
use bench_api::CancellationToken;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use tokio::io::AsyncRead;
use tokio::net::TcpListener;

const SCRIPT: &[u8] = b"#!/bin/sh\necho fio-3.39\n";
const SCRIPT_SHA: &str = "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8";

/// A file of three byte ranges, whose bytes differ from place to place.
fn big() -> &'static [u8] {
    static BIG: OnceLock<Vec<u8>> = OnceLock::new();
    BIG.get_or_init(|| {
        (0..3 * MIN_RANGE)
            .map(|index| (index % 251) as u8)
            .collect()
    })
}

/// The requests for a byte range of [`big`] that the server answered.
static BIG_RANGES: AtomicUsize = AtomicUsize::new(0);

/// The requests for the script at `/flaky-script`.
static FLAKY_SCRIPT: AtomicUsize = AtomicUsize::new(0);

/// The byte ranges of [`big`] asked for at `/flaky`, as first and last byte, in order.
static FLAKY_RANGES: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// `response`, an answer with a body of `body` bytes, cut off in the middle of its body, as a
/// connection that breaks off would leave it.
fn broken(mut response: Vec<u8>, body: usize) -> Vec<u8> {
    response.truncate(response.len() - body + body / 2);
    response
}

/// Serves `GET /fio` with the script, ignoring any byte range; `GET /ranged` with the script, or
/// the byte range that the request asks for, and `GET /big` likewise with [`big`]; `GET /wrong`
/// with another range than any asked for; `GET /moved` with a redirect to `/ranged`; `GET /cut`
/// with a body that breaks off; `GET /status/<code>` with that status; and everything else with
/// 404. `GET /shifted` and `GET /shorter` answer the range asked for one byte off, at its start or
/// at its end. `GET /flaky-script` breaks off in the middle of the script the first time, and
/// `GET /flaky` in the middle of each range of [`big`] the first time it is asked for.
async fn serve() -> String {
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a local test server");
    let base = format!(
        "http://{}",
        listener
            .local_addr()
            .expect("bound listener has an address")
    );
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = vec![0_u8; 4096];
                let read = socket.read(&mut request).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&request[..read]).into_owned();
                let response = if request.starts_with("GET /fio ") {
                    whole(SCRIPT)
                } else if request.starts_with("GET /ranged ") {
                    match asked_range(&request) {
                        Some((first, last)) => partial(SCRIPT, first, last),
                        None => whole(SCRIPT),
                    }
                } else if request.starts_with("GET /big ") {
                    match asked_range(&request) {
                        Some((first, last)) => {
                            BIG_RANGES.fetch_add(1, Ordering::SeqCst);
                            partial(big(), first, last)
                        }
                        None => whole(big()),
                    }
                } else if request.starts_with("GET /flaky-script ") {
                    let response = whole(SCRIPT);
                    if FLAKY_SCRIPT.fetch_add(1, Ordering::SeqCst) == 0 {
                        broken(response, SCRIPT.len())
                    } else {
                        response
                    }
                } else if let Some((first, last)) = asked_range(&request)
                    && request.starts_with("GET /flaky ")
                {
                    let response = partial(big(), first, last);
                    let mut asked = FLAKY_RANGES
                        .lock()
                        .expect("no test panics while holding it");
                    let again = asked.iter().any(|(_, ends)| *ends == last);
                    asked.push((first, last));
                    if again {
                        response
                    } else {
                        broken(response, last + 1 - first)
                    }
                } else if request.starts_with("GET /wrong ") {
                    partial(SCRIPT, 0, 0)
                } else if let Some((first, last)) = asked_range(&request)
                    && request.starts_with("GET /shifted ")
                {
                    partial(SCRIPT, first + 1, last)
                } else if let Some((first, last)) = asked_range(&request)
                    && request.starts_with("GET /shorter ")
                {
                    partial(SCRIPT, first, last - 1)
                } else if request.starts_with("GET /moved ") {
                    b"HTTP/1.1 302 Found\r\nLocation: /ranged\r\nContent-Length: 0\r\n\
                      Connection: close\r\n\r\n"
                        .to_vec()
                } else if let Some((code, _)) = request
                    .strip_prefix("GET /status/")
                    .and_then(|rest| rest.split_once(' '))
                {
                    format!(
                        "HTTP/1.1 {code} Status\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .into_bytes()
                } else if request.starts_with("GET /cut ") {
                    b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\npartial"
                        .to_vec()
                } else {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_vec()
                };
                let _ = socket.write_all(&response).await;
            });
        }
    });
    base
}

/// A 200 answer with `body`.
fn whole(body: &[u8]) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    [head.as_bytes(), body].concat()
}

/// A 206 answer with the bytes `first` to `last` of `body`.
fn partial(body: &[u8], first: usize, last: usize) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {first}-{last}/{}\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len(),
        last + 1 - first
    );
    [head.as_bytes(), &body[first..=last]].concat()
}

/// The first and last byte of a request's `Range: bytes=first-last` header.
fn asked_range(request: &str) -> Option<(usize, usize)> {
    let line = request
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("range: bytes="))?;
    let (first, last) = line["range: bytes=".len()..].trim().split_once('-')?;
    Some((first.parse().ok()?, last.parse().ok()?))
}

/// What a download's body holds.
async fn read(mut body: impl AsyncRead + Unpin) -> Vec<u8> {
    use tokio::io::AsyncReadExt;

    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes)
        .await
        .expect("the body reads whole");
    bytes
}

#[tokio::test]
async fn fetcher_streams_bodies_and_turns_failures_into_reasons() {
    use tokio::io::AsyncReadExt;

    let base = serve().await;
    let fetcher = Fetcher::try_new().unwrap();

    let Ok(Part::Whole(body)) = fetcher.open(&format!("{base}/fio"), None).await else {
        panic!("download should succeed")
    };
    assert_eq!(read(body).await, SCRIPT);

    let Err(not_found) = fetcher.open(&format!("{base}/missing"), None).await else {
        panic!("404 should fail")
    };
    assert_eq!(
        not_found,
        DownloadError::Final("download failed: HTTP 404".to_owned())
    );

    let Err(DownloadError::Transient(refused)) = fetcher.open("http://127.0.0.1:1/fio", None).await
    else {
        panic!("a closed port should fail, in a way that may pass")
    };
    assert!(refused.starts_with("download failed: "), "{refused}");
    assert!(
        refused.contains("tcp connect error: Connection refused"),
        "the reason names its cause: {refused}"
    );
    let hint = fetcher::proxy_hint(|name| std::env::var(name).ok());
    assert!(refused.ends_with(&hint), "{refused}");

    let Ok(Part::Whole(mut cut)) = fetcher.open(&format!("{base}/cut"), None).await else {
        panic!("the response should start")
    };
    let Err(broken) = cut.read_to_end(&mut Vec::new()).await else {
        panic!("a body that breaks off should fail")
    };
    assert!(broken.to_string().ends_with(&hint), "{broken}");
    assert!(
        broken
            .to_string()
            .contains(": end of file before message length reached"),
        "the reason names its cause: {broken}"
    );
}

#[test]
fn an_error_names_each_of_its_causes_once() {
    /// An error with a text and a cause.
    #[derive(Debug)]
    struct Layer(&'static str, Option<Box<Layer>>);
    impl std::fmt::Display for Layer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for Layer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.1
                .as_deref()
                .map(|cause| cause as &(dyn std::error::Error + 'static))
        }
    }
    let layer = |text, cause: Option<Layer>| Layer(text, cause.map(Box::new));

    let refused = layer(
        "error sending request",
        Some(layer(
            "tcp connect error",
            Some(layer("Connection refused", None)),
        )),
    );
    assert_eq!(
        fetcher::causes(&refused),
        "error sending request: tcp connect error: Connection refused"
    );
    // A cause that the message quotes already, then one more below it.
    let reset = layer(
        "connection error: Connection reset by peer",
        Some(layer(
            "Connection reset by peer",
            Some(layer("os error 104", None)),
        )),
    );
    assert_eq!(
        fetcher::causes(&reset),
        "connection error: Connection reset by peer: os error 104"
    );
    assert_eq!(fetcher::causes(&layer("alone", None)), "alone");
}

#[tokio::test]
async fn a_byte_range_comes_back_as_asked_or_the_whole_file_from_a_server_that_ignores_it() {
    let base = serve().await;
    let fetcher = Fetcher::try_new().unwrap();

    let Ok(Part::Range(body)) = fetcher.open(&format!("{base}/ranged"), Some(2..9)).await else {
        panic!("the server serves ranges")
    };
    assert_eq!(read(body).await, &SCRIPT[2..9]);

    let Ok(Part::Range(body)) = fetcher.open(&format!("{base}/moved"), Some(5..7)).await else {
        panic!("a redirect keeps the range")
    };
    assert_eq!(read(body).await, &SCRIPT[5..7]);

    let Ok(Part::Whole(body)) = fetcher.open(&format!("{base}/fio"), Some(2..9)).await else {
        panic!("a server that ignores ranges sends the whole file")
    };
    assert_eq!(read(body).await, SCRIPT);

    let wrong = [
        ("wrong", Some(2..9)),
        ("wrong", None),
        ("shifted", Some(2..9)),
        ("shorter", Some(2..9)),
    ];
    for (path, range) in wrong {
        let Err(wrong) = fetcher.open(&format!("{base}/{path}"), range.clone()).await else {
            panic!("{path} {range:?}: a range other than the one asked for is no download")
        };
        assert_eq!(
            wrong,
            DownloadError::Final(
                "download failed: the server sent another byte range than the one asked for"
                    .to_owned()
            )
        );
    }
}

#[tokio::test]
async fn the_answers_of_a_busy_or_failing_server_may_pass_and_the_others_will_not() {
    let base = serve().await;
    let fetcher = Fetcher::try_new().unwrap();

    let answers = [
        (403, false),
        (404, false),
        (416, false),
        (408, true),
        (429, true),
        (500, true),
        (502, true),
        (503, true),
        (504, true),
    ];
    for (code, may_pass) in answers {
        let Err(error) = fetcher.open(&format!("{base}/status/{code}"), None).await else {
            panic!("{code} should fail")
        };
        let reason = format!("download failed: HTTP {code}");
        let expected = if may_pass {
            DownloadError::Transient(reason)
        } else {
            DownloadError::Final(reason)
        };
        assert_eq!(error, expected);
    }
}

#[test]
fn the_proxy_hint_names_the_variable_that_is_set_or_how_to_set_one() {
    let unset = fetcher::proxy_hint(|_| None);
    assert_eq!(unset, "; set HTTPS_PROXY or ALL_PROXY to use a proxy");
    assert_eq!(
        fetcher::proxy_hint(|_| Some(String::new())),
        unset,
        "an empty variable counts as unset"
    );
    assert_eq!(
        fetcher::proxy_hint(|name| (name == "HTTP_PROXY").then(|| "http://proxy:3128".to_owned())),
        unset,
        "HTTP_PROXY applies to http:// addresses only"
    );
    // With every variable from one on set, that one is named: HTTPS_PROXY comes first, then
    // https_proxy, ALL_PROXY and all_proxy. The value, which may hold a password, never shows.
    let order = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"];
    for (first, name) in order.iter().enumerate() {
        let hint = fetcher::proxy_hint(|variable| {
            order[first..]
                .contains(&variable)
                .then(|| "socks5h://alice:s3cret@proxy:1080".to_owned())
        });
        assert_eq!(
            hint,
            format!("; {name} is set on this host: check that the proxy works")
        );
    }
}

/// An executable tool named `name`, pinned to `body` at `url`.
fn tool(name: &'static str, url: String, body: &[u8]) -> ToolSpec {
    use sha2::Digest;
    use std::fmt::Write;

    let mut sha256 = String::new();
    for byte in sha2::Sha256::digest(body) {
        write!(sha256, "{byte:02x}").expect("write to a string");
    }
    let assets: &'static [ToolAsset] = Box::leak(Box::new([ToolAsset {
        arch: std::env::consts::ARCH,
        url: Box::leak(url.into_boxed_str()),
        sha256: Box::leak(sha256.into_boxed_str()),
        size: body.len() as u64,
    }]));
    ToolSpec {
        name,
        version: "1.0",
        raw_sockets: false,
        form: ToolForm::Executable,
        assets,
    }
}

#[tokio::test]
async fn provisioning_through_the_real_client_installs_verified_tools_whole_or_in_ranges() {
    let base = serve().await;
    let tools = [
        tool("fio", format!("{base}/fio"), SCRIPT),
        tool("big", format!("{base}/big"), big()),
    ];
    assert_eq!(tools[0].assets[0].sha256, SCRIPT_SHA);
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Fetcher::try_new().unwrap();

    let provisioned = provision::provision_all(
        &tools,
        std::env::consts::ARCH,
        dir.path(),
        32,
        &CancellationToken::new(),
        |url, range| fetcher.open(url, range),
    )
    .await;

    assert!(provisioned.failed.is_empty(), "{:?}", provisioned.failed);
    assert_eq!(std::fs::read(&provisioned.ready["fio"]).unwrap(), SCRIPT);
    assert_eq!(std::fs::read(&provisioned.ready["big"]).unwrap(), big());
    assert_eq!(
        BIG_RANGES.load(Ordering::SeqCst),
        3,
        "one request for each range"
    );
}

#[tokio::test]
async fn provisioning_through_the_real_client_goes_on_where_a_download_broke_off() {
    let base = serve().await;
    let tools = [
        tool("fio", format!("{base}/flaky-script"), SCRIPT),
        tool("big", format!("{base}/flaky"), big()),
    ];
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Fetcher::try_new().unwrap();

    let provisioned = provision::provision_all(
        &tools,
        std::env::consts::ARCH,
        dir.path(),
        32,
        &CancellationToken::new(),
        |url, range| fetcher.open(url, range),
    )
    .await;

    assert!(provisioned.failed.is_empty(), "{:?}", provisioned.failed);
    assert_eq!(std::fs::read(&provisioned.ready["fio"]).unwrap(), SCRIPT);
    assert_eq!(std::fs::read(&provisioned.ready["big"]).unwrap(), big());
    assert_eq!(
        FLAKY_SCRIPT.load(Ordering::SeqCst),
        2,
        "the whole file started over once"
    );
    let asked = FLAKY_RANGES
        .lock()
        .expect("no test panics while holding it")
        .clone();
    assert_eq!(asked.len(), 6, "each range twice: {asked:?}");
    let range = usize::try_from(MIN_RANGE).expect("in memory");
    for index in 0..3 {
        let (first, last) = (index * range, (index + 1) * range - 1);
        assert!(asked.contains(&(first, last)), "{asked:?}");
        let again = asked
            .iter()
            .find(|(start, ends)| *ends == last && *start != first);
        assert!(
            again.is_some_and(|(start, _)| (first + 1..=first + range / 2).contains(start)),
            "range {index} went on after the bytes that had come: {asked:?}"
        );
    }
}
