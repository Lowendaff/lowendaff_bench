//! The measurement with recorded runs instead of iperf3: the families and servers in YABS's order,
//! each run's command, the tries on other ports with their waits, a server that cannot be reached,
//! what it returns, publishes and logs, its failures and a cancellation.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Progress;
use bench_api::Step;
use bench_api::Stop;
use bench_host::profile::HostProfile;
use bench_tool::job;
use bench_tool::job::Job;
use bench_tool::job::RunError;
use bench_tool::job::User;
use plugin_network::iperf3::Failure;
use plugin_network::iperf3::Output;
use plugin_network::iperf3::measure;
use plugin_network::iperf3::result::Settings;
use plugin_network::iperf3::result::Speed;
use plugin_network::iperf3::result::Tested;
use plugin_network::iperf3::result::Way;
use plugin_network::iperf3::servers::Chosen;
use plugin_network::iperf3::servers::Server;
use tokio::time::Instant;

const IPERF3: &str = "/tools/iperf3";

/// What a measurement came to.
struct Measured {
    result: Result<Output, Stop<Failure>>,
    published: Option<Output>,
    steps: Vec<Step>,
    jobs: Vec<Job>,
    took: Duration,
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/iperf3/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path}: {err}"))
}

fn finished(code: i32, name: &str) -> job::Output {
    job::Output {
        code: Some(code),
        text: fixture(name),
    }
}

#[allow(clippy::unnecessary_wraps)]
fn upload() -> Result<job::Output, RunError> {
    Ok(finished(0, "upload.json"))
}

#[allow(clippy::unnecessary_wraps)]
fn download() -> Result<job::Output, RunError> {
    Ok(finished(0, "download.json"))
}

#[allow(clippy::unnecessary_wraps)]
fn busy() -> Result<job::Output, RunError> {
    Ok(finished(1, "busy.json"))
}

#[allow(clippy::unnecessary_wraps)]
fn refused() -> Result<job::Output, RunError> {
    Ok(finished(1, "refused.json"))
}

/// The upload of `upload.json`, on try `tries`.
fn uploaded(tries: u32) -> Way {
    Way::Measured(Speed {
        bits: 283_115_282_384,
        tries,
        rtt: Some(8),
        retransmits: Some(60),
    })
}

/// The download of `download.json`, on try `tries`.
fn downloaded(tries: u32) -> Way {
    Way::Measured(Speed {
        bits: 213_527_856_726,
        tries,
        rtt: None,
        retransmits: None,
    })
}

fn server(id: &str, location: &str, country: &str, ports: (u16, u16), ipv6: bool) -> Server {
    Server {
        id: id.to_owned(),
        country: country.to_owned(),
        location: location.to_owned(),
        provider: "Leaseweb".to_owned(),
        host: format!("{id}.example.net"),
        ports,
        ipv4: true,
        ipv6,
        link: "10G".to_owned(),
        default: true,
    }
}

fn tokyo() -> Server {
    server("tokyo", "Tokyo", "JP", (5201, 5210), true)
}

fn frankfurt() -> Server {
    server("frankfurt", "Frankfurt", "DE", (5200, 5209), false)
}

fn chosen(servers: Vec<Server>) -> Chosen {
    Chosen {
        servers,
        unknown: Vec::new(),
    }
}

/// Measures `chosen` over `families` as `settings` say, taking the first port not tried yet; the
/// runs return `script` in order, and cancel when `cancel_at` runs have started.
async fn measure_with(
    chosen: Chosen,
    families: (bool, bool),
    settings: Settings,
    script: Vec<Result<job::Output, RunError>>,
    cancel_at: Option<usize>,
) -> Measured {
    let cancel = CancellationToken::new();
    let (ctx, mut channels) = Context::<Output>::new(
        Arc::new(HostProfile::default()),
        Arc::new(BTreeMap::new()),
        Arc::new(BTreeMap::new()),
        Instant::now() + Duration::from_secs(45 * 60),
        "/run/work".into(),
        cancel.clone(),
    );
    let script = Mutex::new(VecDeque::from(script));
    let jobs = Mutex::new(Vec::new());
    let started = Instant::now();
    let result = measure::measure(
        &ctx,
        Path::new(IPERF3),
        chosen,
        families,
        settings,
        |_| 0,
        |job| {
            let count = {
                let mut jobs = jobs.lock().expect("no test panics while holding the jobs");
                jobs.push(job);
                jobs.len()
            };
            if cancel_at == Some(count) {
                cancel.cancel();
            }
            let next = script
                .lock()
                .expect("no test panics while holding the script")
                .pop_front()
                .expect("the script has a run for each job");
            async move { next }
        },
    )
    .await;
    let took = started.elapsed();
    let mut steps = Vec::new();
    while let Ok(progress) = channels.progress.try_recv() {
        if let Progress::Step(step) = progress {
            steps.push(step);
        }
    }
    let published = channels.published.borrow().clone();
    Measured {
        result,
        published,
        steps,
        jobs: jobs
            .into_inner()
            .expect("no test panicked while holding the jobs"),
        took,
    }
}

/// The arguments of the jobs: the family, the host, the port and whether it is a download.
fn runs(jobs: &[Job]) -> Vec<String> {
    jobs.iter()
        .map(|job| {
            let args: Vec<String> = job
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            let way = if args.contains(&"-R".to_owned()) {
                "down"
            } else {
                "up"
            };
            format!("{} {}:{} {way}", args[0], args[2], args[4])
        })
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[tokio::test(start_paused = true)]
async fn each_server_gets_an_upload_a_pause_and_a_download_over_ipv4_then_ipv6() {
    let measured = measure_with(
        chosen(vec![tokyo(), frankfurt()]),
        (true, true),
        Settings::YABS,
        vec![
            upload(),
            download(),
            upload(),
            download(),
            upload(),
            download(),
        ],
        None,
    )
    .await;

    assert_eq!(
        runs(&measured.jobs),
        strings(&[
            "-4 tokyo.example.net:5201 up",
            "-4 tokyo.example.net:5201 down",
            "-4 frankfurt.example.net:5200 up",
            "-4 frankfurt.example.net:5200 down",
            "-6 tokyo.example.net:5201 up",
            "-6 tokyo.example.net:5201 down",
        ]),
        "Frankfurt has no IPv6"
    );
    let tested = |location: &str, country: &str| Tested {
        location: location.to_owned(),
        country: country.to_owned(),
        provider: "Leaseweb".to_owned(),
        upload: uploaded(1),
        download: downloaded(1),
    };
    let output = Output {
        version: Some("3.22".to_owned()),
        settings: Settings::YABS,
        unknown: Vec::new(),
        ipv4: vec![tested("Tokyo", "JP"), tested("Frankfurt", "DE")],
        ipv6: vec![tested("Tokyo", "JP")],
    };
    assert_eq!(measured.result, Ok(output.clone()));
    assert_eq!(
        measured.published,
        Some(output),
        "published after each server"
    );
    assert_eq!(
        measured
            .result
            .as_ref()
            .map(|output| output.ipv4[0].latency()),
        Ok(Some(8))
    );
    assert_eq!(
        measured.took,
        Duration::from_secs(3),
        "a second's pause before each download"
    );
    let step = |upload: bool, location: &'static str, country: &'static str, ipv6: bool| {
        if upload {
            Step::Upload {
                location: Cow::Borrowed(location),
                country: Cow::Borrowed(country),
                ipv6,
            }
        } else {
            Step::Download {
                location: Cow::Borrowed(location),
                country: Cow::Borrowed(country),
                ipv6,
            }
        }
    };
    assert_eq!(
        measured.steps,
        [
            step(true, "Tokyo", "JP", false),
            step(false, "Tokyo", "JP", false),
            step(true, "Frankfurt", "DE", false),
            step(false, "Frankfurt", "DE", false),
            step(true, "Tokyo", "JP", true),
            step(false, "Tokyo", "JP", true),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn iperf3_runs_as_nobody_from_the_root_directory_its_output_never_quoted() {
    let measured = measure_with(
        chosen(vec![tokyo()]),
        (true, false),
        Settings::YABS,
        vec![upload(), download()],
        None,
    )
    .await;

    let mut upload = Job::new(
        IPERF3,
        strings(&[
            "-4",
            "-c",
            "tokyo.example.net",
            "-p",
            "5201",
            "-P",
            "8",
            "-t",
            "10",
            "-J",
            "--connect-timeout",
            "5000",
        ]),
        Duration::from_secs(20),
    );
    upload.dir = Some(PathBuf::from("/"));
    upload.quotable = false;
    upload.user = Some(User::NOBODY);
    let mut download = upload.clone();
    download.args.push("-R".into());
    assert_eq!(measured.jobs, [upload, download]);
}

#[tokio::test(start_paused = true)]
async fn the_seconds_streams_and_limit_of_the_settings_go_to_iperf3() {
    let settings = Settings {
        seconds: 3,
        streams: 2,
        rate: Some(100),
    };
    let measured = measure_with(
        chosen(vec![tokyo()]),
        (true, false),
        settings,
        vec![upload(), download()],
        None,
    )
    .await;

    let args: Vec<String> = measured.jobs[1]
        .args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        strings(&[
            "-4",
            "-c",
            "tokyo.example.net",
            "-p",
            "5201",
            "-P",
            "2",
            "-t",
            "3",
            "-J",
            "--connect-timeout",
            "5000",
            "-b",
            "100M",
            "-R",
        ])
    );
    assert_eq!(measured.jobs[1].limit, Duration::from_secs(13));
    assert_eq!(measured.result.map(|output| output.settings), Ok(settings));
}

#[tokio::test(start_paused = true)]
async fn a_failed_try_goes_to_a_port_not_tried_yet_2_seconds_later_up_to_three_times() {
    let measured = measure_with(
        chosen(vec![tokyo()]),
        (true, false),
        Settings::YABS,
        vec![busy(), busy(), upload(), refused(), download()],
        None,
    )
    .await;

    assert_eq!(
        runs(&measured.jobs),
        strings(&[
            "-4 tokyo.example.net:5201 up",
            "-4 tokyo.example.net:5202 up",
            "-4 tokyo.example.net:5203 up",
            "-4 tokyo.example.net:5201 down",
            "-4 tokyo.example.net:5202 down",
        ]),
        "each direction starts again from the server's ports"
    );
    let output = measured.result.expect("a speed each way");
    assert_eq!(output.ipv4[0].upload, uploaded(3));
    assert_eq!(output.ipv4[0].download, downloaded(2));
    assert_eq!(
        measured.took,
        Duration::from_secs(2 + 2 + 1 + 2),
        "two waits, the pause, a wait"
    );
}

#[tokio::test(start_paused = true)]
async fn a_server_unreachable_on_every_try_is_not_asked_for_its_download() {
    let measured = measure_with(
        chosen(vec![tokyo(), frankfurt()]),
        (true, false),
        Settings::YABS,
        vec![refused(), refused(), refused(), upload(), download()],
        None,
    )
    .await;

    assert_eq!(measured.jobs.len(), 5, "no download from Tokyo");
    let output = measured.result.expect("Frankfurt's speeds");
    assert_eq!(output.ipv4[0].upload, Way::Unreachable);
    assert_eq!(output.ipv4[0].download, Way::Skipped);
    assert_eq!(output.ipv4[0].latency(), None);
    assert_eq!(output.ipv4[1].upload, uploaded(1));
}

#[tokio::test(start_paused = true)]
async fn a_server_reached_on_one_try_is_not_unreachable_and_keeps_that_tries_reason() {
    let measured = measure_with(
        chosen(vec![tokyo()]),
        (true, false),
        Settings::YABS,
        vec![refused(), busy(), refused(), download()],
        None,
    )
    .await;

    let output = measured.result.expect("the download's speed");
    assert_eq!(output.ipv4[0].upload, Way::Busy);
    assert_eq!(
        output.ipv4[0].download,
        downloaded(1),
        "the download is tried"
    );
}

#[tokio::test(start_paused = true)]
async fn a_server_of_one_port_gets_one_try() {
    let one = server("one", "Osaka", "JP", (5201, 5201), false);
    let measured = measure_with(
        chosen(vec![one]),
        (true, false),
        Settings::YABS,
        vec![busy(), busy()],
        None,
    )
    .await;

    assert_eq!(
        runs(&measured.jobs),
        strings(&["-4 one.example.net:5201 up", "-4 one.example.net:5201 down"])
    );
    assert_eq!(measured.result, Err(Stop::Failed(Failure::NoResult)));
    let published = measured.published.expect("the server's reasons");
    assert_eq!(published.ipv4[0].upload, Way::Busy);
    assert_eq!(published.ipv4[0].download, Way::Busy);
}

#[tokio::test(start_paused = true)]
async fn a_run_that_goes_wrong_keeps_the_runs_reason_and_iperf3s_errors_their_text() {
    let error = Ok(job::Output {
        code: Some(1),
        text: r#"{"start": {}, "end": {}, "error": "control socket has closed unexpectedly"}"#
            .to_owned(),
    });
    let timed_out = Err(RunError::Fault(job::Fault::TimedOut(Duration::from_secs(
        20,
    ))));
    let measured = measure_with(
        chosen(vec![tokyo()]),
        (true, false),
        Settings::YABS,
        vec![busy(), busy(), error, refused(), refused(), timed_out],
        None,
    )
    .await;

    let published = measured.published.expect("the server's reasons");
    assert_eq!(
        published.ipv4[0].upload,
        Way::Error("control socket has closed unexpectedly".to_owned())
    );
    assert_eq!(
        published.ipv4[0].download,
        Way::Fault(job::Fault::TimedOut(Duration::from_secs(20)))
    );
}

#[tokio::test(start_paused = true)]
async fn the_countries_without_a_server_go_into_the_result() {
    let measured = measure_with(
        Chosen {
            servers: vec![tokyo()],
            unknown: strings(&["BR"]),
        },
        (true, false),
        Settings::YABS,
        vec![upload(), download()],
        None,
    )
    .await;

    assert_eq!(
        measured.result.map(|output| output.unknown),
        Ok(strings(&["BR"]))
    );
}

#[tokio::test(start_paused = true)]
async fn a_host_without_the_families_of_the_servers_skips() {
    let measured = measure_with(
        chosen(vec![frankfurt()]),
        (false, true),
        Settings::YABS,
        Vec::new(),
        None,
    )
    .await;

    assert_eq!(measured.result, Err(Stop::Skipped(Failure::NoFamily)));
    assert!(measured.jobs.is_empty());
    assert_eq!(measured.published, None);
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_run_stops_the_item_with_the_servers_done_before_it() {
    let measured = measure_with(
        chosen(vec![tokyo(), frankfurt()]),
        (true, false),
        Settings::YABS,
        vec![upload(), download(), upload(), Err(RunError::Cancelled)],
        Some(4),
    )
    .await;

    assert_eq!(measured.result, Err(Stop::Failed(Failure::Cancelled)));
    assert_eq!(measured.jobs.len(), 4);
    let published = measured.published.expect("Tokyo's speeds");
    assert_eq!(published.ipv4.len(), 1, "Frankfurt did not end");
    assert_eq!(published.ipv4[0].download, downloaded(1));
}
