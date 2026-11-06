//! Command-line flow: argument parsing, selection, config file, and a full run through
//! the terminal renderer with the run directory removed afterwards.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use bench::app;
use bench::app::Cli;
use bench::app::EXIT_INTERRUPTED;
use bench_api::CancellationToken;
use bench_api::Context;
use bench_api::Descriptor;
use bench_api::Env;
use bench_api::Measurement;
use bench_api::Plugin;
use bench_api::PluginError;
use bench_api::Record;
use bench_api::Registration;
use bench_api::Requirements;
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Good {
    samples: u32,
}

impl Plugin for Good {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.good",
        title: "示例测试",
        version: "0.1.0",
        requires: Requirements::NONE,
        timeout: Duration::from_secs(10),
    };

    async fn run(&self, ctx: &Context) -> Result<(), PluginError> {
        ctx.emit(Record::metric(
            "rate",
            "速率",
            Measurement::try_new(f64::from(self.samples), "MB/s")?,
        ));
        Ok(())
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Other {}

impl Plugin for Other {
    const DESCRIPTOR: Descriptor = Descriptor {
        id: "test.other",
        title: "另一个测试",
        ..Good::DESCRIPTOR
    };

    async fn run(&self, _ctx: &Context) -> Result<(), PluginError> {
        Ok(())
    }
}

const REGISTRY: &[Registration] = &[
    bench_api::register::<Good>(),
    bench_api::register::<Other>(),
];

fn cli(args: &[&str]) -> Cli {
    use clap::Parser;

    Cli::try_parse_from(std::iter::once("lowendaff-bench").chain(args.iter().copied()))
        .expect("test arguments must parse")
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn ids_are_comma_separated_and_work_dir_defaults_to_cwd() {
    let cli = cli(&["--only", "test.good,test.other", "--skip", "test.other"]);
    assert_eq!(cli.only, ["test.good", "test.other"]);
    assert_eq!(cli.skip, ["test.other"]);
    assert_eq!(cli.work_dir, PathBuf::from("."));
}

#[test]
fn selection_keeps_registry_order_and_rejects_unknown_ids() {
    assert_eq!(
        app::select(REGISTRY, &[], &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(REGISTRY, &ids(&["test.other", "test.good"]), &[]).unwrap(),
        ["test.good", "test.other"]
    );
    assert_eq!(
        app::select(REGISTRY, &[], &ids(&["test.good"])).unwrap(),
        ["test.other"]
    );
    assert_eq!(
        app::select(REGISTRY, &ids(&["nope"]), &ids(&["nada"])).unwrap_err(),
        ["未知测试项 `nope`", "未知测试项 `nada`"]
    );
}

#[test]
fn config_file_holds_plugin_tables_only() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, body: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        path
    };

    let good = write("good.toml", "[plugins.\"test.good\"]\nsamples = 2\n");
    let expected = BTreeMap::from([("test.good".to_owned(), serde_json::json!({ "samples": 2 }))]);
    assert_eq!(app::load_config(Some(&good)).unwrap(), expected);

    let stray = write("stray.toml", "timeout = 3\n");
    assert_eq!(
        app::load_config(Some(&stray)).unwrap_err(),
        "配置文件中有未知的项 `timeout`，只支持 [plugins.\"<id>\"]"
    );

    let broken = write("broken.toml", "[plugins\n");
    assert!(
        app::load_config(Some(&broken))
            .unwrap_err()
            .starts_with("无法解析配置文件")
    );

    let bad_value = write("bad.toml", "[plugins.\"test.good\"]\nsamples = \"many\"\n");
    let config = bad_value.to_string_lossy().into_owned();
    let Err(errors) = app::prepare(&cli(&["--config", &config]), REGISTRY) else {
        panic!("bad config must not plan")
    };
    assert!(
        errors[0].starts_with("测试项 test.good 的配置无效："),
        "{errors:?}"
    );
}

#[tokio::test]
async fn a_run_prints_sections_and_summary_then_removes_its_directory() {
    let work = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("bench.toml");
    std::fs::write(&config, "[plugins.\"test.good\"]\nsamples = 3\n").unwrap();
    let planned = app::prepare(
        &cli(&[
            "--skip",
            "test.other",
            "--config",
            &config.to_string_lossy(),
        ]),
        REGISTRY,
    )
    .unwrap();
    let mut out = Vec::new();

    let code = app::execute(
        planned,
        Env::default(),
        work.path(),
        &CancellationToken::new(),
        &mut out,
    )
    .await
    .unwrap();

    let text = String::from_utf8(out).unwrap();
    assert_eq!(code, 0);
    assert!(
        text.starts_with(&format!(
            "lowendaff-bench {}\n\n【示例测试】\n",
            env!("CARGO_PKG_VERSION")
        )),
        "{text}"
    );
    assert!(text.contains("  速率              3 MB/s\n"), "{text}");
    assert!(
        text.contains("\n共 1 项：完成 1，跳过 0，失败 0，超时 0，中断 0。总用时 "),
        "{text}"
    );
    assert_eq!(
        std::fs::read_dir(work.path()).unwrap().count(),
        0,
        "run directory must be removed"
    );
}

#[tokio::test]
async fn an_interrupted_run_exits_130_and_still_lists_every_plugin() {
    let work = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let planned = app::prepare(&cli(&[]), REGISTRY).unwrap();
    let mut out = Vec::new();

    let code = app::execute(planned, Env::default(), work.path(), &cancel, &mut out)
        .await
        .unwrap();

    let text = String::from_utf8(out).unwrap();
    assert_eq!(code, EXIT_INTERRUPTED);
    assert!(text.contains("【示例测试】\n  已中断\n"), "{text}");
    assert!(text.contains("【另一个测试】\n  已中断\n"), "{text}");
    assert!(
        text.contains("共 2 项：完成 0，跳过 0，失败 0，超时 0，中断 2。"),
        "{text}"
    );
}
