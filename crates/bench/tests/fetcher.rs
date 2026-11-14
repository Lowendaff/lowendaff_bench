//! The download adapter against a local plain-HTTP server: streaming, HTTP errors,
//! connection errors and the proxy hint they end in, and provisioning through the real client.
//! Building the client also proves the preconfigured rustls config matches reqwest's rustls
//! version. The tests expect no proxy variables, as on CI's runners.

use bench::fetcher;
use bench::fetcher::Fetcher;
use bench::provision;
use bench::provision::Part;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use tokio::io::AsyncRead;
use tokio::net::TcpListener;

const SCRIPT: &[u8] = b"#!/bin/sh\necho fio-3.39\n";
const SCRIPT_SHA: &str = "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8";

/// Serves `GET /fio` with the script, ignoring any byte range; `GET /ranged` with the script, or
/// the byte range that the request asks for; `GET /wrong` with another range than any asked for;
/// `GET /moved` with a redirect to `/ranged`; `GET /cut` with a body that breaks off; and
/// everything else with 404. `GET /shifted` and `GET /shorter` answer the range asked for one
/// byte off, at its start or at its end.
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
    assert_eq!(not_found, "下载失败：HTTP 404");

    let Err(refused) = fetcher.open("http://127.0.0.1:1/fio", None).await else {
        panic!("closed port should fail")
    };
    assert!(refused.starts_with("下载失败："), "{refused}");
    let hint = fetcher::proxy_hint(|name| std::env::var(name).ok());
    assert!(refused.ends_with(&hint), "{refused}");

    let Ok(Part::Whole(mut cut)) = fetcher.open(&format!("{base}/cut"), None).await else {
        panic!("the response should start")
    };
    let Err(broken) = cut.read_to_end(&mut Vec::new()).await else {
        panic!("a body that breaks off should fail")
    };
    assert!(broken.to_string().ends_with(&hint), "{broken}");
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
        assert_eq!(wrong, "下载失败：服务器返回的不是所要的那一段");
    }
}

#[test]
fn the_proxy_hint_names_the_variable_that_is_set_or_how_to_set_one() {
    let unset = fetcher::proxy_hint(|_| None);
    assert_eq!(unset, "；可设置环境变量 HTTPS_PROXY 或 ALL_PROXY 使用代理");
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
        assert_eq!(hint, format!("；本机设置了 {name}，请确认这个代理可用"));
    }
}

#[tokio::test]
async fn provisioning_through_the_real_client_installs_a_verified_tool() {
    let url: &'static str = Box::leak(format!("{}/fio", serve().await).into_boxed_str());
    let assets: &'static [ToolAsset] = Box::leak(Box::new([ToolAsset {
        arch: std::env::consts::ARCH,
        url,
        sha256: SCRIPT_SHA,
        size: SCRIPT.len() as u64,
    }]));
    let fio = ToolSpec {
        name: "fio",
        version: "3.39",
        form: ToolForm::Executable,
        assets,
    };
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Fetcher::try_new().unwrap();

    let provisioned =
        provision::provision_all(&[fio], std::env::consts::ARCH, dir.path(), |url, range| {
            fetcher.open(url, range)
        })
        .await;

    assert!(provisioned.failed.is_empty(), "{:?}", provisioned.failed);
    assert_eq!(std::fs::read(&provisioned.ready["fio"]).unwrap(), SCRIPT);
}
