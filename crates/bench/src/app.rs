//! Command-line flow: parse, select, configure, provision, run, summarize.
//! `main` adds the root check, the runtime and Ctrl-C handling around it.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bench_api::CancellationToken;
use bench_api::Env;
use bench_api::Registration;
use bench_host::profile::HostProfile;

use crate::fetcher::Fetcher;
use crate::orchestrator;
use crate::orchestrator::Planned;
use crate::orchestrator::Settings;
use crate::progress::Log;
use crate::provision;
use crate::report;
use crate::report::terminal::Page;
use crate::results::Measured;

/// Exit code for usage, config or environment errors, and for not running as root.
pub const EXIT_USAGE: u8 = 2;
/// Exit code after Ctrl-C; the results so far were printed.
pub const EXIT_INTERRUPTED: u8 = 130;

const GRACE: Duration = Duration::from_secs(5);

/// Linux 服务器综合测试，需要 root 权限运行。
#[derive(Debug, clap::Parser)]
#[command(name = "lowendaff-bench", version)]
pub struct Cli {
    /// 只运行这些测试项（逗号分隔的 id 或分组，如 cpu,disk.fio）
    #[arg(long, value_delimiter = ',')]
    pub only: Vec<String>,

    /// 跳过这些测试项（逗号分隔的 id 或分组）
    #[arg(long, value_delimiter = ',')]
    pub skip: Vec<String>,

    /// 配置文件（TOML），每个测试项一节：[plugins."<id>"]
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// 在此目录下创建运行目录，结束时删除（不要使用 /tmp）
    #[arg(long, default_value = ".")]
    pub work_dir: PathBuf,

    /// 列出全部测试项后退出
    #[arg(long)]
    pub list: bool,

    /// 在进度日志中显示更多细节，如每个工具的下载地址
    #[arg(short, long)]
    pub verbose: bool,

    /// 在报告中显示完整的公网地址（默认只显示前一部分）
    #[arg(long)]
    pub show_ip: bool,
}

impl Cli {
    /// Parses the process arguments; exits with usage help on errors.
    #[must_use]
    pub fn from_env() -> Self {
        use clap::Parser;

        Self::parse()
    }
}

/// Prints every registered plugin with its id, title, platforms and pinned tools.
pub fn list(registry: &[Registration], out: &mut impl Write) {
    let _ = writeln!(out, "共 {} 个测试项", registry.len());
    for registration in registry {
        let descriptor = registration.descriptor;
        let platforms: Vec<String> = descriptor
            .requires
            .platforms
            .iter()
            .map(ToString::to_string)
            .collect();
        let platforms = if platforms.is_empty() {
            String::new()
        } else {
            format!("（适用：{}）", platforms.join("、"))
        };
        let tools: Vec<String> = descriptor
            .requires
            .tools
            .iter()
            .map(|tool| format!("{} {}", tool.name, tool.version))
            .collect();
        let tools = if tools.is_empty() {
            String::new()
        } else {
            format!("（工具：{}）", tools.join("、"))
        };
        let _ = writeln!(
            out,
            "  {}  {}{platforms}{tools}",
            descriptor.id, descriptor.title
        );
    }
}

/// Builds every selected plugin; all problems are reported together and nothing runs.
pub fn prepare(
    cli: &Cli,
    registry: &[Registration],
) -> Result<Vec<Planned<Measured>>, Vec<String>> {
    let config = load_config(cli.config.as_deref()).map_err(|err| vec![err])?;
    let ids = select(registry, &cli.only, &cli.skip)?;
    orchestrator::plan(registry, &ids, &config)
}

/// Registry order, narrowed by `--only` and `--skip`, which take ids or group names; unknown
/// names are errors.
pub fn select(
    registry: &[Registration],
    only: &[String],
    skip: &[String],
) -> Result<Vec<&'static str>, Vec<String>> {
    // A name is a whole id or a whole group, never a part of one.
    let names = |name: &str, id: &str| name == id || name == orchestrator::group_of(id);
    let known = |name: &String| {
        registry
            .iter()
            .any(|registration| names(name, registration.descriptor.id))
    };
    let errors: Vec<String> = only
        .iter()
        .chain(skip)
        .filter(|name| !known(name))
        .map(|name| format!("未知测试项或分组 `{name}`"))
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(registry
        .iter()
        .map(|registration| registration.descriptor.id)
        .filter(|id| only.is_empty() || only.iter().any(|wanted| names(wanted, id)))
        .filter(|id| !skip.iter().any(|skipped| names(skipped, id)))
        .collect())
}

/// Reads `[plugins."<id>"]` tables; any other top-level key is an error.
pub fn load_config(path: Option<&Path>) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    let text = fs::read_to_string(path)
        .map_err(|err| format!("无法读取配置文件 {}：{err}", path.display()))?;
    let mut table: toml::Table = toml::from_str(&text)
        .map_err(|err| format!("无法解析配置文件 {}：{err}", path.display()))?;
    let plugins = table.remove("plugins");
    if let Some(key) = table.keys().next() {
        return Err(format!(
            "配置文件中有未知的项 `{key}`，只支持 [plugins.\"<id>\"]"
        ));
    }
    let Some(plugins) = plugins else {
        return Ok(BTreeMap::new());
    };
    let toml::Value::Table(plugins) = plugins else {
        return Err("配置文件中的 plugins 必须是表".to_owned());
    };
    let mut config = BTreeMap::new();
    for (id, value) in plugins {
        let value =
            serde_json::to_value(value).map_err(|err| format!("测试项 {id} 的配置无效：{err}"))?;
        config.insert(id, value);
    }
    Ok(config)
}

/// Runs a prepared plan on the host that `host` describes, inside a fresh run directory under
/// `work_dir`, which is removed afterwards, and writes the report to `out`: its tables wrap to
/// `width`, and the public addresses show in full with `show_ip`. Returns the process exit code.
pub async fn execute(
    planned: Vec<Planned<Measured>>,
    host: HostProfile,
    work_dir: &Path,
    cancel: &CancellationToken,
    mut out: impl Write,
    width: Option<u16>,
    show_ip: bool,
) -> Result<u8, String> {
    let started = Instant::now();
    let run_dir = tempfile::Builder::new()
        .prefix("lowendaff-bench-")
        .tempdir_in(work_dir)
        .map_err(|err| format!("无法在 {} 创建运行目录：{err}", work_dir.display()))?;
    // The plugins of the first contract see the host through `Env`.
    let mut env = Env {
        arch: host.arch.clone(),
        ipv4: host.network.ipv4.reachable(),
        ipv6: host.network.ipv6.reachable(),
        tools: BTreeMap::new(),
        platform: host.platform,
        libc: host.libc,
    };
    let (planned, not_applicable) = orchestrator::for_platform(planned, host.platform);

    // Only the tools of items this host can run, so that nothing is downloaded in vain, such
    // as Geekbench on a host without glibc or IPv4.
    let runnable = planned
        .iter()
        .map(|planned| *planned.requires())
        .filter(|requires| orchestrator::check_host(requires, &env).is_ok());
    let tools = provision::required_tools(runnable);
    let mut failed_tools = BTreeMap::new();
    if !tools.is_empty() {
        let tools_dir = run_dir.path().join("tools");
        fs::create_dir(&tools_dir).map_err(|err| format!("无法创建工具目录：{err}"))?;
        let fetcher = Fetcher::try_new()?;
        let provisioned =
            provision::provision_all(&tools, &env.arch, &tools_dir, |url| fetcher.open(url)).await;
        env.tools = provisioned.ready;
        failed_tools = provisioned.failed;
    }

    let host = Arc::new(host);
    let settings = Settings {
        grace: GRACE,
        work_root: run_dir.path().to_path_buf(),
        origin: tokio::time::Instant::from_std(started),
        failed_tools,
        host: Arc::clone(&host),
    };
    let runs = orchestrator::run_plan(planned, Arc::new(env), cancel, &settings, &mut Log).await;
    let page = Page {
        version: env!("CARGO_PKG_VERSION"),
        host: &host,
        show_ip,
        not_applicable: &not_applicable,
        runs: &runs,
        total: started.elapsed(),
    };
    // Output is best effort: a closed stdout must not change the exit code.
    let _ = out.write_all(report::terminal::render(&page, width).as_bytes());
    drop(run_dir);
    Ok(if cancel.is_cancelled() {
        EXIT_INTERRUPTED
    } else {
        0
    })
}
