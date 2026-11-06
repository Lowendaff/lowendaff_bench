//! Command-line flow (design §5): parse, select, configure, provision, run, summarize.
//! `main` adds the root check, the runtime and Ctrl-C handling around it.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use bench_api::CancellationToken;
use bench_api::Env;
use bench_api::Registration;
use clap::Parser;

use crate::fetch::Fetcher;
use crate::orchestrator::Planned;
use crate::orchestrator::Settings;
use crate::orchestrator::plan;
use crate::orchestrator::run_plan;
use crate::provision::provision_all;
use crate::provision::required_tools;
use crate::render::Terminal;

/// Exit code for usage, config or environment errors, and for not running as root.
pub const EXIT_USAGE: u8 = 2;
/// Exit code after Ctrl-C; the results so far were printed.
pub const EXIT_INTERRUPTED: u8 = 130;

const GRACE: Duration = Duration::from_secs(5);

/// Linux 服务器综合测试，需要 root 权限运行。
#[derive(Debug, Parser)]
#[command(name = "lowendaff-bench", version)]
pub struct Cli {
    /// 只运行这些测试项（逗号分隔的 id）
    #[arg(long, value_delimiter = ',')]
    pub only: Vec<String>,

    /// 跳过这些测试项（逗号分隔的 id）
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
}

pub fn list(registry: &[Registration], out: &mut impl Write) {
    let _ = writeln!(out, "共 {} 个测试项", registry.len());
    for reg in registry {
        let d = reg.descriptor;
        let tools: Vec<String> = d
            .requires
            .tools
            .iter()
            .map(|t| format!("{} {}", t.name, t.version))
            .collect();
        let tools = if tools.is_empty() {
            String::new()
        } else {
            format!("（工具：{}）", tools.join("、"))
        };
        let _ = writeln!(out, "  {}  {}{tools}", d.id, d.title);
    }
}

/// Builds every selected plugin; all problems are reported together and nothing runs.
pub fn prepare(cli: &Cli, registry: &[Registration]) -> Result<Vec<Planned>, Vec<String>> {
    let config = load_config(cli.config.as_deref()).map_err(|err| vec![err])?;
    let ids = select(registry, &cli.only, &cli.skip)?;
    plan(registry, &ids, &config)
}

/// Registry order, narrowed by `--only` and `--skip`; unknown ids are errors.
pub fn select(
    registry: &[Registration],
    only: &[String],
    skip: &[String],
) -> Result<Vec<&'static str>, Vec<String>> {
    let known = |id: &String| registry.iter().any(|reg| reg.descriptor.id == id.as_str());
    let errors: Vec<String> = only
        .iter()
        .chain(skip)
        .filter(|id| !known(id))
        .map(|id| format!("未知测试项 `{id}`"))
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(registry
        .iter()
        .map(|reg| reg.descriptor.id)
        .filter(|id| only.is_empty() || only.iter().any(|o| o == id))
        .filter(|id| !skip.iter().any(|s| s == id))
        .collect())
}

/// Reads `[plugins."<id>"]` tables; any other top-level key is an error.
pub fn load_config(path: Option<&Path>) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let Some(path) = path else {
        return Ok(BTreeMap::new());
    };
    let text = std::fs::read_to_string(path)
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
    plugins
        .into_iter()
        .map(|(id, value)| {
            let value = serde_json::to_value(value)
                .map_err(|err| format!("测试项 {id} 的配置无效：{err}"))?;
            Ok((id, value))
        })
        .collect()
}

/// Runs a prepared plan inside a fresh run directory under `work_dir`, which is removed
/// afterwards. Returns the process exit code.
pub async fn execute(
    planned: Vec<Planned>,
    mut env: Env,
    work_dir: &Path,
    cancel: &CancellationToken,
    out: impl Write,
) -> Result<u8, String> {
    let started = Instant::now();
    let run_dir = tempfile::Builder::new()
        .prefix("lowendaff-bench-")
        .tempdir_in(work_dir)
        .map_err(|err| format!("无法在 {} 创建运行目录：{err}", work_dir.display()))?;
    let mut terminal = Terminal::new(out);
    terminal.header(env!("CARGO_PKG_VERSION"));

    let tools = required_tools(planned.iter().map(Planned::descriptor));
    if !tools.is_empty() {
        let tools_dir = run_dir.path().join("tools");
        std::fs::create_dir(&tools_dir).map_err(|err| format!("无法创建工具目录：{err}"))?;
        let fetcher = Fetcher::new()?;
        let provisioned =
            provision_all(&tools, &env.arch, &tools_dir, |url| fetcher.open(url)).await;
        terminal.tools(&tools, &provisioned);
        env.tools = provisioned.ready;
    }

    let settings = Settings {
        grace: GRACE,
        work_root: run_dir.path().to_path_buf(),
    };
    let runs = run_plan(planned, Arc::new(env), cancel, &settings, &mut terminal).await;
    terminal.summary(&runs, started.elapsed());
    drop(run_dir);
    Ok(if cancel.is_cancelled() {
        EXIT_INTERRUPTED
    } else {
        0
    })
}

impl Cli {
    pub fn from_env() -> Self {
        Self::parse()
    }
}
