//! 简体中文：交互式终端界面（TUI）文案。

use std::path::Path;

use super::SimplifiedChinese;
use crate::locale::TuiWords;

impl TuiWords for SimplifiedChinese {
    fn tui_config_title(&self, version: &str) -> String {
        format!("lowendaff-bench {version}  交互式配置")
    }

    fn tui_running_title(&self, version: &str, elapsed: &str) -> String {
        format!("lowendaff-bench {version}  正在运行 {elapsed}")
    }

    fn tui_results_title(&self, version: &str) -> String {
        format!("lowendaff-bench {version}  报告查看器")
    }

    fn tui_enabled_badge(&self, enabled: usize, total: usize) -> String {
        format!("已启用 {enabled}/{total}")
    }

    fn tui_terminal_too_small(&self, cols: u16, rows: u16) -> String {
        format!("终端窗口（{cols}x{rows}）过小；请至少调整至 60x16 或按 q 退出")
    }

    fn tui_items_pane(&self) -> &'static str {
        "测试项目与全局选项"
    }

    fn tui_start_action(&self, enabled: usize) -> String {
        format!("开始运行测试（已选 {enabled} 项）")
    }

    fn tui_global_row(&self) -> &'static str {
        "运行与报告全局选项"
    }

    fn tui_overview_pane(&self) -> &'static str {
        "运行概览"
    }

    fn tui_global_pane(&self) -> &'static str {
        "全局运行与报告选项"
    }

    fn tui_params_pane(&self, id: &str) -> String {
        format!("项目参数设置 ({id})")
    }

    fn tui_overview_lines(
        &self,
        enabled: usize,
        total: usize,
        customized: usize,
        format: &str,
        output: &str,
        work_dir: &str,
    ) -> Vec<String> {
        vec![
            format!("已选测试项：  {enabled} / {total}"),
            format!("已自定义项：  {customized}"),
            format!("报告格式：    {format}"),
            format!("输出文件：    {output}"),
            format!("工作目录：    {work_dir}"),
            String::new(),
            "按 Enter 或 r 按标准顺序启动所选测试项目。".to_owned(),
            "按向下键开关单项，或按向右/Tab 调整各单项与全局参数。".to_owned(),
            "快捷键：g 切换整组分类；a 全选/全不选；d 恢复默认；L 切换语言。".to_owned(),
        ]
    }

    fn tui_item_meta(&self, group: &str, timeout_mins: u64) -> String {
        format!("分类：{group}  |  超时上限：{timeout_mins} 分钟")
    }

    fn tui_item_requires(&self, requirements: &str) -> String {
        format!("运行要求：{requirements}")
    }

    fn tui_any_host(&self) -> &'static str {
        "所有 Linux 主机"
    }

    fn tui_req_ipv4(&self) -> &'static str {
        "IPv4"
    }

    fn tui_req_ipv6(&self) -> &'static str {
        "IPv6"
    }

    fn tui_req_internet(&self) -> &'static str {
        "公网连接"
    }

    fn tui_req_numa(&self) -> &'static str {
        "多 NUMA 节点"
    }

    fn tui_req_glibc(&self, major: u32, minor: u32) -> String {
        format!("glibc >= {major}.{minor}")
    }

    fn tui_fixed_params_note(&self) -> &'static str {
        "该项使用标准化固定参数，确保不同服务器之间的测试得分可直接横向对比。"
    }

    fn tui_yes(&self) -> &'static str {
        "是"
    }

    fn tui_no(&self) -> &'static str {
        "否"
    }

    fn tui_field_enabled(&self) -> &'static str {
        "启用状态"
    }

    fn tui_field_cpu_suite(&self, suite: &str) -> String {
        let name = match suite {
            "sysbench" => "sysbench（素数筛查）",
            "7zip" => "7-Zip（LZMA2 MIPS）",
            "unixbench" => "UnixBench（12 项综合指数）",
            "hpl" => "HPL（OpenBLAS GFLOPS）",
            "passmark" => "PassMark（CPU Mark）",
            "ycruncher" => "y-cruncher（圆周率 Pi，仅 x86_64）",
            "prime95" => "Prime95（Small FFT 烤机，仅 x86_64）",
            "geekbench" => "Geekbench 7",
            other => other,
        };
        format!("子套件：{name}")
    }

    fn tui_field_countries(&self) -> &'static str {
        "国家/地区 (ISO / all)"
    }

    fn tui_field_servers(&self) -> &'static str {
        "指定节点 ID（覆盖）"
    }

    fn tui_field_seconds(&self) -> &'static str {
        "单向测速时长（秒）"
    }

    fn tui_field_streams(&self) -> &'static str {
        "并发流数"
    }

    fn tui_field_rate(&self) -> &'static str {
        "单流限速 (Mbit/s)"
    }

    fn tui_field_taier_browse(&self) -> &'static str {
        "选择测速节点（树形）"
    }

    fn tui_field_taier_targets(&self) -> &'static str {
        "目标选择器"
    }

    fn tui_taier_browse_value(&self, selections: &str) -> String {
        let count = selections
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .count();
        if count == 0 {
            "[Enter] 浏览省市与运营商节点（默认北上广三网）".to_owned()
        } else {
            format!("[Enter] 浏览省市与运营商节点（已选 {count} 组选择器）")
        }
    }

    fn tui_taier_picker_title(&self) -> &'static str {
        "中国三网测速节点选择器 (network.taier)"
    }

    fn tui_taier_picker_root(&self) -> &'static str {
        "全部区域"
    }

    fn tui_taier_picker_loading(&self) -> &'static str {
        "正在拉取中国信通院全球网测节点列表...（按 Esc 取消）"
    }

    fn tui_taier_picker_error_hint(&self) -> &'static str {
        "Enter / r：重试拉取  |  Esc：关闭"
    }

    fn tui_taier_picker_tip(&self) -> &'static str {
        "直辖市直接展开运营商，省份展开下辖城市；按 / 搜索过滤。"
    }

    fn tui_taier_picker_keys(&self) -> &'static str {
        "Space:勾选  Enter/右:展开  左:返回  /:搜索  a:全选  d:默认  c:清空  s/Esc:应用  q:取消"
    }

    fn tui_taier_operator_label(&self, code: &str) -> &'static str {
        match code {
            "telecom" => "中国电信 (ct)",
            "unicom" => "中国联通 (cu)",
            "mobile" => "中国移动 (cm)",
            "cernet" => "教育网 CERNET (edu)",
            "cbn" => "中国广电 (cbn)",
            "other" => "其他 / 境外 (other)",
            _ => "运营商",
        }
    }

    fn tui_taier_picker_empty_err(&self) -> String {
        "请至少勾选一个运营商节点（按 d 恢复默认，或按 q 取消）".to_owned()
    }

    fn tui_field_cloudflare_concurrency(&self) -> &'static str {
        "并发工作数"
    }

    fn tui_field_cloudflare_udp(&self) -> &'static str {
        "STUN UDP 丢包探测"
    }

    fn tui_field_inetspeed_threads(&self) -> &'static str {
        "多线程工作数"
    }

    fn tui_field_inetspeed_timeout(&self) -> &'static str {
        "单线程超时（秒）"
    }

    fn tui_field_inetspeed_max(&self) -> &'static str {
        "单线程流量上限"
    }

    fn tui_field_inetspeed_latency_count(&self) -> &'static str {
        "空载延迟采样数"
    }

    fn tui_field_inetspeed_endpoint(&self) -> &'static str {
        "固定节点 IP"
    }

    fn tui_field_tcping_group(&self, group: &str) -> String {
        format!("探测分组：{group}")
    }

    fn tui_field_tcping_all(&self) -> &'static str {
        "展开全部 31 省与全球节点"
    }

    fn tui_field_tcping_count(&self) -> &'static str {
        "单节点握手探测次数"
    }

    fn tui_field_trace_group(&self, group: &str) -> String {
        format!("路由分组：{group}")
    }

    fn tui_field_targets(&self) -> &'static str {
        "指定目标 ID（覆盖）"
    }

    fn tui_field_geo(&self) -> &'static str {
        "跳点 ASN 与归属地查询"
    }

    fn tui_field_trace_tcp(&self) -> &'static str {
        "中国大陆 TCP 大包追踪"
    }

    fn tui_field_trace_packet_size(&self) -> &'static str {
        "TCP 探测包大小（字节）"
    }

    fn tui_field_bgp_upload(&self) -> &'static str {
        "上传拓扑图生成外链"
    }

    fn tui_field_dnsbl(&self) -> &'static str {
        "DNSBL 黑名单查询 (25)"
    }

    fn tui_field_mail(&self) -> &'static str {
        "25 端口邮局连通性"
    }

    fn tui_field_media_group(&self, group: &str) -> String {
        format!("检测区域：{group}")
    }

    fn tui_field_services(&self) -> &'static str {
        "指定平台 ID（覆盖）"
    }

    fn tui_field_language(&self) -> &'static str {
        "界面与报告语言"
    }

    fn tui_lang_picker_title(&self) -> &'static str {
        "选择界面与报告语言"
    }

    fn tui_lang_picker_keys(&self) -> &'static str {
        "上下键: 移动  Enter/Space: 确认切换  Esc/q: 取消"
    }

    fn tui_field_format(&self) -> &'static str {
        "报告格式"
    }

    fn tui_field_output(&self) -> &'static str {
        "保存报告至文件"
    }

    fn tui_field_work_dir(&self) -> &'static str {
        "工作目录"
    }

    fn tui_field_show_ip(&self) -> &'static str {
        "公网 IP 展示方式"
    }

    fn tui_field_connections(&self) -> &'static str {
        "下载并发连接数"
    }

    fn tui_field_socks5_proxy(&self) -> &'static str {
        "SOCKS5 下载代理"
    }

    fn tui_field_github_proxy(&self) -> &'static str {
        "GitHub 镜像代理前缀"
    }

    fn tui_field_verbose(&self) -> &'static str {
        "详细调试日志"
    }

    fn tui_default_locations(&self) -> &'static str {
        "默认（5 个精选地区）"
    }

    fn tui_default_taier_cities(&self) -> &'static str {
        "默认（北京、上海、广州）"
    }

    fn tui_default_inetspeed_endpoint(&self) -> &'static str {
        "自动（双 DoH 优选最低 RTT）"
    }

    fn tui_default_none(&self) -> &'static str {
        "无"
    }

    fn tui_unlimited(&self) -> &'static str {
        "不限速"
    }

    fn tui_default_cpu_note(&self) -> &'static str {
        "y-cruncher 与 Prime95 默认按需开启（仅支持 x86_64，ARM64 自动跳过）。"
    }

    fn tui_default_taier_note(&self) -> &'static str {
        "默认测试北京、上海、广州的电信、联通、移动三网节点。"
    }

    fn tui_default_cloudflare_note(&self) -> &'static str {
        "测量 speed.cloudflare.com 双栈吞吐、缓冲膨胀评级与 STUN UDP 丢包。"
    }

    fn tui_default_inetspeed_note(&self) -> &'static str {
        "通过双 DoH 优选最佳 mensura.cdn-apple.com 节点并执行单线程与多线程测速。"
    }

    fn tui_default_tcping_note(&self) -> &'static str {
        "默认探测精选三网省会、CN2/9929/CMIN2 精品网与全球核心枢纽（可展开 31 省全量节点）。"
    }

    fn tui_default_trace_note(&self) -> &'static str {
        "未勾选分组时默认测试最近视频缓存与精选运营商线路。"
    }

    fn tui_default_bgp_note(&self) -> &'static str {
        "在 text 模式下上传脱敏 AS 路径 SVG 至 imageupload.app 生成图片链接。"
    }

    fn tui_default_media_note(&self) -> &'static str {
        "未勾选区域时默认检测全球五大区域全部平台。"
    }

    fn tui_masked(&self) -> &'static str {
        "否（默认脱敏）"
    }

    fn tui_unmasked(&self) -> &'static str {
        "是（显示完整 IP）"
    }

    fn tui_edit_title(&self, field: &str) -> String {
        format!("编辑：{field}")
    }

    fn tui_edit_hint(&self) -> &'static str {
        "Enter: 确认  |  Ctrl-U: 清空（恢复默认）  |  Esc: 取消"
    }

    fn tui_no_items_selected(&self) -> String {
        "请至少启用一个测试项目后再开始运行".to_owned()
    }

    fn tui_no_cpu_suites_selected(&self) -> String {
        "请在 cpu.bench 中至少勾选一个 CPU 子套件（或禁用 cpu.bench）".to_owned()
    }

    fn tui_invalid_number(&self, field: &str, min: u32, max: u32) -> String {
        format!("{field} 必须是 {min} 到 {max} 之间的整数")
    }

    fn tui_empty_work_dir(&self) -> String {
        "工作目录不能为空（当前目录请填 .）".to_owned()
    }

    fn tui_all_items_and_options(&self) -> &'static str {
        "全部项目与运行选项"
    }

    fn tui_reset_done(&self, target: &str) -> String {
        format!("已将 {target} 恢复为默认配置")
    }

    fn tui_gauge_completed(&self, done: usize, total: usize) -> String {
        format!("已完成 {done} / {total}")
    }

    fn tui_taier_picker_badge(&self, selected: usize, est_secs: u64) -> String {
        let est_text = if est_secs >= 60 {
            format!("~{}分 {:02}秒", est_secs / 60, est_secs % 60)
        } else {
            format!("~{est_secs}秒")
        };
        format!("已选 {selected} 组 ({est_text})")
    }

    fn tui_taier_filter_prefix(&self) -> &'static str {
        "搜索过滤："
    }

    fn tui_taier_filter_active(&self, filter: &str) -> String {
        format!("当前过滤：\"{filter}\"（/: 编辑，Esc: 清除）")
    }

    fn tui_taier_no_matches(&self) -> &'static str {
        "无匹配地区（按 Esc 清除过滤条件）"
    }

    fn tui_taier_targets_preview(&self, targets: Option<&str>) -> String {
        match targets {
            Some(text) => format!("已选目标：{text}"),
            None => "已选目标：（未选择）".to_owned(),
        }
    }

    fn tui_taier_region_meta(&self, municipality: bool, overseas: bool, cities: usize) -> String {
        if municipality {
            "  （直辖市）".to_owned()
        } else if overseas {
            format!("  （{cities} 个境外节点）")
        } else {
            format!("  （{cities} 个城市）")
        }
    }

    fn tui_taier_node_count(&self, count: usize) -> String {
        if count == 1 {
            "1 个节点".to_owned()
        } else {
            format!("{count} 个候选节点")
        }
    }

    fn tui_keys_config_list(&self) -> &'static str {
        "上下:移动  Space:开关  右/Tab:参数  g:整组  a:全选  d:重置  L:语言  r:运行  q:退出"
    }

    fn tui_keys_config_params(&self) -> &'static str {
        "上下:选项  Space/Enter:切换/编辑  左右:调整  Esc/Tab:返回  d:重置  L:语言  r:运行"
    }

    fn tui_progress_pane(&self, done: usize, total: usize) -> String {
        format!("测试进度 ({done}/{total})")
    }

    fn tui_log_pane(&self, following: bool) -> &'static str {
        if following {
            "实时事件日志（自动跟随）"
        } else {
            "实时事件日志（已暂停 - 按 End 跟随最新）"
        }
    }

    fn tui_collecting_host(&self) -> &'static str {
        "正在采集主机硬件信息并探测 IPv4/IPv6 连通性..."
    }

    fn tui_state_queued(&self) -> &'static str {
        "排队中"
    }

    fn tui_state_running(&self) -> &'static str {
        "运行中"
    }

    fn tui_state_not_applicable(&self) -> &'static str {
        "当前主机不适用"
    }

    fn tui_stopping_banner(&self) -> &'static str {
        "正在安全停止当前测试以生成已完成部分报告... 再次按 Ctrl-C 立即退出"
    }

    fn tui_keys_running(&self) -> &'static str {
        "上下/PgUp/PgDn: 滚动日志  End: 跟随最新  Ctrl-C: 停止并查看已完成报告"
    }

    fn tui_sections_pane(&self) -> &'static str {
        "章节导航"
    }

    fn tui_section_top(&self) -> &'static str {
        "顶部 / 概览"
    }

    fn tui_report_pane(&self, format: &str, wrapped: bool, show_ip: bool) -> String {
        let wrap = if format == "text" {
            if wrapped {
                "，自动折行"
            } else {
                "，不折行"
            }
        } else {
            ""
        };
        let ip = if show_ip { "完整 IP" } else { "脱敏 IP" };
        format!("测试报告（{format}{wrap}，{ip}）")
    }

    fn tui_save_prompt_title(&self) -> &'static str {
        "保存报告到文件"
    }

    fn tui_saved_to(&self, path: &Path) -> String {
        format!("报告已保存至 {}", path.display())
    }

    fn tui_keys_results(&self) -> &'static str {
        "方向键/PgUp/PgDn:滚动  [/]:章节  b:侧栏  f:格式  w:折行  i:IP  L:语言  s:保存  \
         p:打印退出  n:新测  q:退出"
    }
}
