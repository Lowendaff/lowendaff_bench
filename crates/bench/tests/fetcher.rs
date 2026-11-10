//! The download adapter against a local plain-HTTP server: streaming, HTTP errors,
//! connection errors and the proxy hint they end in, and provisioning through the real client.
//! Building the client also proves the preconfigured rustls config matches reqwest's rustls
//! version. The tests expect no proxy variables, as on CI's runners.

use bench::fetcher;
use bench::fetcher::Fetcher;
use bench::provision;
use bench_api::ToolAsset;
use bench_api::ToolForm;
use bench_api::ToolSpec;
use tokio::net::TcpListener;

const SCRIPT: &[u8] = b"#!/bin/sh\necho fio-3.39\n";
const SCRIPT_SHA: &str = "1046784cba2513f64f6ee92655862e88fe249cfbbdb19b5420a08df9cc6540e8";

/// Serves `GET /fio` with the script, `GET /cut` with a body that breaks off, and everything
/// else with 404.
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
                let response = if request[..read].starts_with(b"GET /fio ") {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        SCRIPT.len()
                    );
                    [head.as_bytes(), SCRIPT].concat()
                } else if request[..read].starts_with(b"GET /cut ") {
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

#[tokio::test]
async fn fetcher_streams_bodies_and_turns_failures_into_reasons() {
    use tokio::io::AsyncReadExt;

    let base = serve().await;
    let fetcher = Fetcher::try_new().unwrap();

    let Ok(mut body) = fetcher.open(&format!("{base}/fio")).await else {
        panic!("download should succeed")
    };
    let mut bytes = Vec::new();
    body.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, SCRIPT);

    let Err(not_found) = fetcher.open(&format!("{base}/missing")).await else {
        panic!("404 should fail")
    };
    assert_eq!(not_found, "下载失败：HTTP 404");

    let Err(refused) = fetcher.open("http://127.0.0.1:1/fio").await else {
        panic!("closed port should fail")
    };
    assert!(refused.starts_with("下载失败："), "{refused}");
    let hint = fetcher::proxy_hint(|name| std::env::var(name).ok());
    assert!(refused.ends_with(&hint), "{refused}");

    let Ok(mut cut) = fetcher.open(&format!("{base}/cut")).await else {
        panic!("the response should start")
    };
    let Err(broken) = cut.read_to_end(&mut Vec::new()).await else {
        panic!("a body that breaks off should fail")
    };
    assert!(broken.to_string().ends_with(&hint), "{broken}");
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

    let provisioned = provision::provision_all(&[fio], std::env::consts::ARCH, dir.path(), |url| {
        fetcher.open(url)
    })
    .await;

    assert!(provisioned.failed.is_empty(), "{:?}", provisioned.failed);
    assert_eq!(std::fs::read(&provisioned.ready["fio"]).unwrap(), SCRIPT);
}
