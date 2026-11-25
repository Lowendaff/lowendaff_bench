//! 简体中文：系统概览章节字段与主机硬件描述。

use std::net::IpAddr;
use std::time::Duration;

use bench_host::bmc;
use bench_host::bmc::Bmc;
use bench_host::bmc::InterfaceKind;
use bench_host::cards::Bus;
use bench_host::cards::Duplex;
use bench_host::cards::Link;
use bench_host::cards::MasterKind;
use bench_host::cards::NetworkCard;
use bench_host::cards::Pcie;
use bench_host::cards::PcieLink;
use bench_host::cpu::Model;
use bench_host::cpu::Virtualization;
use bench_host::network::Reach;
use bench_host::network::Why;
use bench_host::platform::ContainerKind;
use bench_host::platform::Hypervisor;
use bench_host::platform::Platform;
use bench_host::platform::PlatformKind;
use bench_host::raid::Attachment;
use bench_host::raid::Card;
use bench_host::smbios::Ecc;
use bench_host::storage::Use;

use super::SimplifiedChinese;
use crate::locale::Cores;
use crate::locale::HostHeader;
use crate::locale::HostLabel;
use crate::locale::HostWords;
use crate::locale::NoEcc;
use crate::locale::NoModules;

const NO_DRIVER: &str = "内核未加载 IPMI 驱动";

impl HostWords for SimplifiedChinese {
    fn host_label(&self, label: HostLabel) -> &'static str {
        match label {
            HostLabel::Hardware => "硬件平台",
            HostLabel::Machine => "整机型号",
            HostLabel::Board => "主板",
            HostLabel::Bios => "BIOS",
            HostLabel::Firmware => "固件",
            HostLabel::Bmc => "BMC",
            HostLabel::Cpu => "处理器",
            HostLabel::CpuModel => "CPU 型号",
            HostLabel::CpuCores => "CPU 核心",
            HostLabel::CpuClock => "CPU 频率",
            HostLabel::CpuCaches => "CPU 缓存",
            HostLabel::Aes => "AES 指令集",
            HostLabel::Virtualization => "硬件虚拟化",
            HostLabel::Memory => "内存",
            HostLabel::MemoryTotal => "内存总量",
            HostLabel::MemoryUsed => "已用内存",
            HostLabel::MemoryUsedEstimated => "已用内存（估算）",
            HostLabel::Swap => "交换空间",
            HostLabel::SwapTotal => "Swap 总量",
            HostLabel::SwapUsed => "已用 Swap",
            HostLabel::Ecc => "ECC 校验",
            HostLabel::Modules => "内存条插槽",
            HostLabel::Raid => "RAID 控制器",
            HostLabel::Disks => "物理磁盘",
            HostLabel::Filesystems => "文件系统",
            HostLabel::Cards => "网络接口",
            HostLabel::Os => "操作系统",
            HostLabel::Kernel => "内核版本",
            HostLabel::Arch => "系统架构",
            HostLabel::Uptime => "运行时间",
            HostLabel::Load => "系统负载",
            HostLabel::Tcp => "TCP 拥塞控制",
        }
    }

    fn host_header(&self, header: HostHeader) -> &'static str {
        match header {
            HostHeader::Device => "设备",
            HostHeader::Interface => "接口",
            HostHeader::Size => "容量",
            HostHeader::Medium => "介质",
            HostHeader::Model => "型号",
            HostHeader::Use => "用途",
            HostHeader::MountPoint => "挂载点",
            HostHeader::Type => "类型",
            HostHeader::Used => "已用",
            HostHeader::Available => "可用",
            HostHeader::OnDisks => "所属磁盘",
            HostHeader::Slot => "插槽",
            HostHeader::Speed => "速率",
            HostHeader::Vendor => "厂商",
            HostHeader::Driver => "驱动",
            HostHeader::Link => "链路状态",
            HostHeader::Mtu => "MTU",
        }
    }

    fn platform(&self, platform: Platform) -> String {
        match platform {
            Platform::Dedicated => "未检测到虚拟化（独立物理机）".to_owned(),
            Platform::Vm(Hypervisor::Qemu) => "QEMU 虚拟机（纯软件模拟）".to_owned(),
            Platform::Vm(hypervisor) => match hypervisor.name() {
                Some(name) => format!("{name} 虚拟机"),
                None => "虚拟机（类型未知）".to_owned(),
            },
            Platform::Container(ContainerKind::Wsl) => "WSL".to_owned(),
            Platform::Container(container) => match container.name() {
                Some(name) => format!("{name} 容器"),
                None => "容器（类型未知）".to_owned(),
            },
        }
    }

    fn platform_kind(&self, kind: PlatformKind) -> &'static str {
        match kind {
            PlatformKind::Dedicated => "独立物理服务器",
            PlatformKind::Vm => "虚拟机",
            PlatformKind::Container => "容器",
        }
    }

    fn read_timed_out(&self) -> String {
        "不可用（读取超时）".to_owned()
    }

    fn none(&self) -> &'static str {
        "无"
    }

    fn aes(&self, supported: bool) -> &'static str {
        if supported { "支持" } else { "不支持" }
    }

    fn virtualization(&self, virtualization: Virtualization) -> &'static str {
        match virtualization {
            Virtualization::IntelVtX => "支持 (VT-x)",
            Virtualization::AmdV => "支持 (AMD-V)",
            Virtualization::Absent => "不支持",
            Virtualization::NotApplicable => "不适用",
        }
    }

    fn cpu_model(&self, model: &Model) -> String {
        match model {
            Model::Name(name) => name.clone(),
            Model::UnknownPart { vendor, part } => format!("{vendor}（部件代码 {part:#05x}）"),
            Model::UnknownVendor {
                implementer,
                part: Some(part),
            } => format!("ARM CPU（厂商代码 {implementer:#04x}，部件代码 {part:#05x}）"),
            Model::UnknownVendor {
                implementer,
                part: None,
            } => format!("ARM CPU（厂商代码 {implementer:#04x}）"),
        }
    }

    fn cores(&self, cores: Cores) -> String {
        match cores {
            Cores::Dedicated {
                threads,
                topology: Some((sockets, cores)),
            } if sockets > 1 => format!("{threads} 线程（{sockets} 路 CPU，共 {cores} 核）"),
            Cores::Dedicated {
                threads,
                topology: Some((_, cores)),
            } => format!("{threads} 线程（{cores} 核）"),
            Cores::Dedicated {
                threads,
                topology: None,
            } => format!("{threads} 线程"),
            Cores::Vm(vcpus) => format!("{vcpus} vCPU"),
            Cores::Container {
                usable,
                quota: Some(quota),
            } => format!("{usable} 核（配额 {}）", decimals(quota)),
            Cores::Container {
                usable,
                quota: None,
            } => format!("{usable} 核"),
        }
    }

    fn with_detail(&self, name: &str, detail: &str) -> String {
        format!("{name} ({detail})")
    }

    fn u_boot(&self, version: &str) -> String {
        format!("U-Boot {version}")
    }

    fn raspberry_pi(&self, built: &str) -> String {
        format!("Raspberry Pi 引导固件（构建于 {built}）")
    }

    fn bmc(&self, bmc: Option<&Bmc>) -> String {
        match bmc {
            Some(Bmc::Driver {
                vendor,
                firmware,
                ipmi,
            }) => {
                let vendor = vendor.map(|number| {
                    bmc::vendor(number).map_or_else(|| format!("厂商 {number}"), str::to_owned)
                });
                let parts: Vec<String> = [
                    vendor,
                    firmware.as_ref().map(|fw| format!("固件 {fw}")),
                    ipmi.as_ref().map(|version| format!("IPMI {version}")),
                ]
                .into_iter()
                .flatten()
                .collect();
                if parts.is_empty() {
                    "已检测到".to_owned()
                } else {
                    parts.join("，")
                }
            }
            Some(Bmc::Interface(interface)) => {
                let parts: Vec<String> = [
                    interface
                        .version
                        .map(|(major, minor)| format!("IPMI {major}.{minor}")),
                    interface.kind.map(|kind| {
                        let name = match kind {
                            InterfaceKind::Kcs => "KCS",
                            InterfaceKind::Smic => "SMIC",
                            InterfaceKind::Bt => "BT",
                            InterfaceKind::Ssif => "SSIF",
                        };
                        format!("{name} 接口")
                    }),
                ]
                .into_iter()
                .flatten()
                .collect();
                if parts.is_empty() {
                    format!("已检测到；接口、厂商与固件不可用：{NO_DRIVER}")
                } else {
                    format!(
                        "已检测到（{}）；厂商与固件不可用：{NO_DRIVER}",
                        parts.join("，")
                    )
                }
            }
            Some(Bmc::Present) => {
                format!("已检测到；接口、厂商与固件不可用：{NO_DRIVER}")
            }
            None => "未检测到".to_owned(),
        }
    }

    fn ecc(&self, modes: &[Ecc], counts: Option<(u64, u64)>) -> String {
        let counts = counts.map(|(corrected, uncorrected)| {
            format!("自启动以来已纠正 {corrected} 次，未纠正 {uncorrected} 次")
        });
        let modes: Vec<&str> = modes
            .iter()
            .map(|mode| match mode {
                Ecc::SingleBit => "支持（单比特纠错）",
                Ecc::MultiBit => "支持（多比特纠错）",
                Ecc::Crc => "支持（CRC）",
                Ecc::Parity => "仅奇偶校验（无纠错）",
                Ecc::None => "无",
                Ecc::Unknown => "未知",
            })
            .collect();
        match (modes.is_empty(), counts) {
            (false, Some(counts)) => format!("{}；{counts}", modes.join("、")),
            (false, None) => format!("{}；错误计数不可用：内核未加载 EDAC 驱动", modes.join("、")),
            (true, Some(counts)) => format!("纠错方式不可用；{counts}"),
            (true, None) => "不可用".to_owned(),
        }
    }

    fn no_ecc(&self, why: NoEcc) -> String {
        let smbios = match why {
            NoEcc::NotInSmbios => "SMBIOS 未提供纠错信息",
            NoEcc::NoSmbios => "系统无 SMBIOS 表",
            NoEcc::SmbiosUnreadable => "无法读取 SMBIOS",
        };
        format!("不可用：{smbios}，且未加载 EDAC 驱动")
    }

    fn no_modules(&self, why: NoModules<'_>) -> String {
        match why {
            NoModules::Board(size) => {
                format!("板载内存 {size}（来自设备树）；无独立内存条信息")
            }
            NoModules::NotInSmbios => "不可用：SMBIOS 未描述内存条插槽".to_owned(),
            NoModules::NoSmbios => "不可用：无法读取 SMBIOS".to_owned(),
            NoModules::DevMem(error) => {
                format!("不可用：无法读取 SMBIOS（/dev/mem：{error}）")
            }
        }
    }

    fn slot(&self, number: usize) -> String {
        format!("插槽 {number}")
    }

    fn empty_slot(&self) -> &'static str {
        "空插槽"
    }

    fn disabled_module(&self) -> &'static str {
        "未启用"
    }

    fn module_speed(&self, configured: Option<u32>, rated: Option<u32>) -> Option<String> {
        match (configured, rated) {
            (Some(configured), Some(rated)) if configured != rated => {
                Some(format!("{configured} MT/s（标称 {rated}）"))
            }
            (Some(configured), _) => Some(format!("{configured} MT/s")),
            (None, Some(rated)) => Some(format!("{rated} MT/s（标称）")),
            (None, None) => None,
        }
    }

    fn disabled_modules(&self, slots: &[String]) -> String {
        format!(
            "{} 安装了固件标记为未启用的内存条（容量报告为 0），总容量未计入",
            slots.join("、")
        )
    }

    fn raid_card(&self, card: &Card) -> String {
        let name = card.name.clone().unwrap_or_else(|| {
            let driver = card.driver.as_ref().map_or_else(
                || "未加载驱动".to_owned(),
                |driver| format!("驱动 {driver}"),
            );
            let ids = format!("PCI {:04x}:{:04x}", card.ids.vendor, card.ids.device);
            match card.ids.subsystem {
                Some((vendor, device)) => {
                    format!("{ids}（子系统 {vendor:04x}:{device:04x}，{driver}）")
                }
                None => format!("{ids}（{driver}）"),
            }
        });
        if card.disks.is_empty() {
            return format!("{name}：未连接磁盘");
        }
        let groups: Vec<String> = [
            (Attachment::Array, "阵列"),
            (Attachment::PassThrough, "直通"),
            (Attachment::Unknown, "未知"),
        ]
        .iter()
        .filter_map(|(attachment, label)| {
            let disks: Vec<&str> = card
                .disks
                .iter()
                .filter(|(_, how)| how == attachment)
                .map(|(disk, _)| disk.as_str())
                .collect();
            (!disks.is_empty()).then(|| format!("{label} {}", disks.join(", ")))
        })
        .collect();
        format!("{name}：{}", groups.join("；"))
    }

    fn medium(&self, rotational: Option<bool>) -> &'static str {
        match rotational {
            Some(false) => "SSD",
            Some(true) => "HDD",
            None => "未知",
        }
    }

    fn uses(&self, uses: &[Use]) -> String {
        if uses.is_empty() {
            return "未挂载".to_owned();
        }
        let parts: Vec<String> = uses
            .iter()
            .map(|item| match item {
                Use::Mount(point) if point == "/" => "系统盘 (/)".to_owned(),
                Use::Mount(point) => point.clone(),
                Use::Swap => "swap".to_owned(),
                Use::ZfsPool(pool) => format!("ZFS 存储池 {pool}"),
            })
            .collect();
        parts.join(", ")
    }

    fn card_link(&self, link: Link) -> String {
        let Link::Up { speed, duplex } = link else {
            return "未连接".to_owned();
        };
        let Some(speed) = speed else {
            return "未上报速率".to_owned();
        };
        let speed = if speed >= 1000 {
            format!("{} Gbit/s", f64::from(speed) / 1000.0)
        } else {
            format!("{speed} Mbit/s")
        };
        match duplex {
            Some(Duplex::Full) => format!("{speed}，全双工"),
            Some(Duplex::Half) => format!("{speed}，半双工"),
            None => speed,
        }
    }

    fn card_uses(&self, card: &NetworkCard) -> String {
        let mut parts = Vec::new();
        if let Some(master) = &card.master {
            parts.push(match master.kind {
                MasterKind::Bridge => format!("网桥 {}", master.name),
                MasterKind::Bond => format!("聚合 {}", master.name),
                MasterKind::Other => format!("从属 {}", master.name),
            });
        }
        match (card.default_route.ipv4, card.default_route.ipv6) {
            (true, true) => parts.push("默认路由 (IPv4, IPv6)".to_owned()),
            (true, false) => parts.push("默认路由 (IPv4)".to_owned()),
            (false, true) => parts.push("默认路由 (IPv6)".to_owned()),
            (false, false) => {}
        }
        if card.virtual_functions > 0 {
            parts.push(format!("{} 个虚拟功能 (VF)", card.virtual_functions));
        }
        if parts.is_empty() {
            "—".to_owned()
        } else {
            parts.join("，")
        }
    }

    fn unnamed_card(&self, bus: &Bus) -> String {
        match bus {
            Bus::Pci { vendor, device } => format!("PCI {vendor:04x}:{device:04x}"),
            Bus::Usb => "USB 网卡".to_owned(),
            Bus::Platform(None) => "主板板载网卡".to_owned(),
            Bus::Other(name) if name.is_empty() => "—".to_owned(),
            Bus::Platform(Some(name)) | Bus::Other(name) => name.clone(),
        }
    }

    fn no_cards(&self, container: bool) -> &'static str {
        if container {
            "容器内不可见物理网卡"
        } else {
            "未检测到网卡"
        }
    }

    fn slow_pcie(&self, interface: &str, pcie: Pcie) -> String {
        let link = |link: PcieLink| format!("PCIe {}.0 x{}", link.generation, link.lanes);
        format!(
            "网卡 {interface} 当前协商在 {}，低于其最高支持的 {}",
            link(pcie.current),
            link(pcie.best)
        )
    }

    fn test_directory(&self) -> &'static str {
        "（测试目录）"
    }

    fn of_the_host(&self) -> &'static str {
        "（宿主机）"
    }

    fn uptime(&self, uptime: Duration) -> String {
        let minutes = usize::try_from(uptime.as_secs() / 60).unwrap_or(usize::MAX);
        let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
        if days > 0 {
            format!("{days} 天 {hours} 小时 {minutes} 分钟")
        } else if hours > 0 {
            format!("{hours} 小时 {minutes} 分钟")
        } else {
            format!("{minutes} 分钟")
        }
    }

    fn tcp(&self, algorithm: &str, qdisc: Option<&str>) -> String {
        match qdisc {
            Some(qdisc) => format!("{algorithm}（队列 {qdisc}）"),
            None => algorithm.to_owned(),
        }
    }

    fn reachable(&self) -> String {
        "可用".to_owned()
    }

    fn public_address(&self, address: &str, source: &str) -> String {
        format!("可用；公网地址 {address}（来自 {source}）")
    }

    fn no_public_address(&self, reasons: &[String]) -> String {
        if reasons.is_empty() {
            return "可用；未能获取公网地址".to_owned();
        }
        format!("可用；未能获取公网地址：{}", reasons.join("，"))
    }

    fn no_route(&self, family: &str) -> String {
        format!("当前主机无 {family} 路由")
    }

    fn reach(&self, ip: IpAddr, reach: Reach) -> String {
        match reach {
            Reach::Connected => format!("已连接 {ip}"),
            Reach::TimedOut => format!("连接 {ip} 超时"),
            Reach::Refused => format!("{ip} 拒绝连接"),
            Reach::NoRoute => format!("无到达 {ip} 的路由"),
            Reach::Failed => format!("无法连接 {ip}"),
        }
    }

    fn why(&self, source: &str, why: Why, family: &str) -> String {
        match why {
            Why::TimedOut => format!("{source} 请求超时"),
            Why::Status(code) => format!("{source} 返回 HTTP {code}"),
            Why::NotAnAddress => format!("{source} 未返回有效的 {family} 地址"),
            Why::NotPublic => format!("{source} 未返回公网地址"),
            Why::NoAddress => format!("{source} 无 {family} 解析地址"),
            Why::Unreachable => format!("无法连接 {source}"),
        }
    }
}

fn decimals(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
