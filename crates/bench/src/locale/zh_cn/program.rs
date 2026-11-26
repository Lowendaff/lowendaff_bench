//! 简体中文：命令行帮助、`--list` 输出及运行前后错误提示。

use std::fmt::Display;
use std::path::Path;

use super::SimplifiedChinese;
use crate::locale::ProgramWords;

impl ProgramWords for SimplifiedChinese {
    fn about(&self) -> String {
        "Linux 服务器综合基准与网络体检工具；需以 root 权限运行\
         （无其他参数时默认启动交互式 TUI 界面）。"
            .to_owned()
    }

    fn help_all(&self) -> String {
        "在非交互命令行模式下运行全部测试项".to_owned()
    }

    fn help_only(&self) -> String {
        "仅运行指定测试项：英文逗号分隔的分类（cpu,memory,disk,network,ip）、\
         项目 ID（如 cpu.bench,disk.fio）或 all"
            .to_owned()
    }

    fn help_skip(&self) -> String {
        "跳过指定测试项：英文逗号分隔的分类、项目 ID 或 all\
         （如 cpu.bench,ip.media）"
            .to_owned()
    }

    fn help_output(&self) -> String {
        "将报告写入指定文件而非标准输出（未指定时打印到终端并在 /tmp 保存归档副本）".to_owned()
    }

    fn help_format(&self) -> String {
        "报告格式：text、markdown（或 md）或 json".to_owned()
    }

    fn help_lang(&self) -> String {
        "界面与报告语言：en（默认）、zh-cn、zh-tw 或 ja（JSON 始终保持英文）".to_owned()
    }

    fn help_work_dir(&self) -> String {
        "在该目录下创建临时运行目录并在结束时自动清理（请避开 /tmp）".to_owned()
    }

    fn help_list(&self) -> String {
        "列出全部测试项后退出".to_owned()
    }

    fn help_verbose(&self) -> String {
        "输出详细调试日志：下载链接与重试、子工具执行命令与退出码".to_owned()
    }

    fn help_show_ip(&self) -> String {
        "在报告中显示完整公网 IP 与反向解析主机名（默认仅显示前缀脱敏形式）".to_owned()
    }

    fn help_download_connections(&self) -> String {
        "单个文件下载的最大并发连接数（1 至 256）".to_owned()
    }

    fn help_socks5_proxy(&self) -> String {
        "仅用于工具与节点列表下载的 SOCKS5 代理（socks5:// 或 socks5h://；\
         绝不用于任何测速与探测）"
            .to_owned()
    }

    fn help_github_proxy(&self) -> String {
        "仅用于 GitHub 工具与列表下载的镜像前缀 URL（例如 https://ghfast.top/）".to_owned()
    }

    fn invalid_socks5_proxy(&self) -> String {
        "无效的 --socks5-proxy：格式应为 socks5://[user:pass@]host[:port] \
         或 socks5h://[user:pass@]host[:port]"
            .to_owned()
    }

    fn invalid_github_proxy(&self) -> String {
        "无效的 --github-proxy：应为不含凭据的 http:// 或 https:// 前缀 URL\
         （例如 https://ghfast.top/）"
            .to_owned()
    }

    fn help_cpu_bench(&self) -> String {
        "cpu.bench 子套件：英文逗号分隔的 sysbench, 7zip, unixbench, hpl, \
         passmark, ycruncher, prime95, geekbench 或 all"
            .to_owned()
    }

    fn help_countries(&self) -> String {
        "network.iperf3 使用的测速国家/地区 ISO 3166 代码（如 US,JP）或 all".to_owned()
    }

    fn help_taier(&self) -> String {
        "network.taier 节点选择器：<地区>[:<运营商>[+<运营商>...]]\
         （如 beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct,all:cm）"
            .to_owned()
    }

    fn help_taier_list(&self) -> String {
        "列出可用的 network.taier 省市区域（或指定省份下的城市）后退出".to_owned()
    }

    fn help_taier_cities(&self) -> String {
        "network.taier 测速城市、省份英文名或节点 ID（如 Beijing,Shanghai,Jiangsu）\
         或 all"
            .to_owned()
    }

    fn help_taier_operators(&self) -> String {
        "network.taier 测速运营商：英文逗号分隔的 telecom, unicom, mobile, \
         cernet, cbn, other 或 all"
            .to_owned()
    }

    fn help_no_cloudflare_udp(&self) -> String {
        "在 network.cloudflare 中跳过 RFC 5389 STUN UDP 丢包探测".to_owned()
    }

    fn help_inetspeed_threads(&self) -> String {
        "network.inetspeed 多线程轮次的并发工作线程数（1 至 64）".to_owned()
    }

    fn help_inetspeed_endpoint(&self) -> String {
        "在 network.inetspeed 中固定指定 Apple CDN IPv4/IPv6 节点 IP（跳过 DoH）".to_owned()
    }

    fn invalid_inetspeed_endpoint(&self, raw: &str) -> String {
        format!("无效的 --inetspeed-endpoint `{raw}`：应为公网 IPv4 或 IPv6 地址")
    }

    fn help_tcping_groups(&self) -> String {
        "network.tcping 探测分组：英文逗号分隔的 china, china-premium, global 或 all".to_owned()
    }

    fn help_tcping_all(&self) -> String {
        "在 network.tcping 中展开测试全部 31 个中国大陆省份三网与次级全球枢纽".to_owned()
    }

    fn help_tcping_count(&self) -> String {
        "network.tcping 单目标 TCP 握手探测次数（5 至 50，默认 10）".to_owned()
    }

    fn help_trace_groups(&self) -> String {
        "network.trace 路由目标分组：英文逗号分隔的 services, china, \
         international 或 all"
            .to_owned()
    }

    fn help_no_trace_tcp(&self) -> String {
        "在 network.trace 中跳过中国大陆目标的 TCP SYN 大包路由追踪".to_owned()
    }

    fn help_trace_packet_size(&self) -> String {
        "network.trace 中国大陆目标 TCP 探测包字节大小（64 至 1500，默认 1024）".to_owned()
    }

    fn help_media_groups(&self) -> String {
        "ip.media 流媒体区域分组：英文逗号分隔的 global, asia, europe, \
         americas, oceania 或 all"
            .to_owned()
    }

    fn help_no_dnsbl(&self) -> String {
        "在 ip.quality 中跳过 DNSBL 黑名单查询".to_owned()
    }

    fn help_no_mail(&self) -> String {
        "在 ip.quality 中跳过出站 TCP 25 端口邮件连通性检测".to_owned()
    }

    fn help_no_bgp_upload(&self) -> String {
        "在 text 模式下跳过上传 BGP 拓扑 SVG 图至 imageupload.app".to_owned()
    }

    fn tui_not_a_tty(&self) -> String {
        "交互式界面需要终端（TTY）；非交互环境请传入 --all 或 --only <项目>".to_owned()
    }

    fn list(&self, items: &[String]) -> String {
        items.join("、")
    }

    fn item_count(&self, count: usize) -> String {
        format!("共 {count} 个测试项")
    }

    fn group_line(&self, title: &str, name: &str) -> String {
        format!("[{title}]（分类：{name}）")
    }

    fn item_line(
        &self,
        id: &str,
        name: &str,
        platforms: &[String],
        numa: bool,
        tools: &[String],
    ) -> String {
        let mut line = format!("  {id}  {name}");
        if !platforms.is_empty() {
            line.push_str(&format!("（适用环境：{}）", self.list(platforms)));
        }
        if numa {
            line.push_str("（适用环境：多 NUMA 节点）");
        }
        if !tools.is_empty() {
            line.push_str(&format!("（外部工具：{}）", self.list(tools)));
        }
        line
    }

    fn unknown_selection(&self, name: &str) -> String {
        format!("未知的测试项或分类 `{name}`")
    }

    fn output_unwritable(&self, path: &Path, error: &dyn Display) -> String {
        format!("无法将报告写入 {}：{error}", path.display())
    }

    fn overrides_invalid(&self, error: &dyn Display) -> String {
        format!("无效的插件覆盖参数：{error}")
    }

    fn config_unknown_item(&self, id: &str) -> String {
        format!("未知测试项 `{id}` 的参数配置")
    }

    fn config_invalid(&self, id: &str, error: &dyn Display) -> String {
        format!("测试项 {id} 参数配置无效：{error}")
    }

    fn unknown_item(&self, id: &str) -> String {
        format!("未知测试项 `{id}`")
    }

    fn not_root(&self) -> String {
        "lowendaff-bench 需要以 root 权限运行（请使用 sudo）".to_owned()
    }

    fn runtime_failed(&self, error: &dyn Display) -> String {
        format!("无法启动异步运行时：{error}")
    }

    fn host_failed(&self, error: &dyn Display) -> String {
        format!("无法读取主机硬件信息：{error}")
    }

    fn network_failed(&self, error: &dyn Display) -> String {
        format!("无法完成网络连通性探测：{error}")
    }

    fn run_dir_failed(&self, dir: &Path, error: &dyn Display) -> String {
        format!("无法在 {} 下创建临时运行目录：{error}", dir.display())
    }

    fn tools_dir_failed(&self, error: &dyn Display) -> String {
        format!("无法创建工具目录：{error}")
    }

    fn http_client_failed(&self, error: &dyn Display) -> String {
        format!("无法初始化 HTTP 客户端：{error}")
    }

    fn tls_failed(&self, error: &dyn Display) -> String {
        format!("无法初始化 TLS 配置：{error}")
    }

    fn panic_without_text(&self) -> String {
        "（无文本消息的异常崩溃）".to_owned()
    }
}
