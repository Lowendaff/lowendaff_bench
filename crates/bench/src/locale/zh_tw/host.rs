//! 繁體中文：系統概覽章節欄位與主機硬體描述。

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

use super::TraditionalChinese;
use crate::locale::Cores;
use crate::locale::HostHeader;
use crate::locale::HostLabel;
use crate::locale::HostWords;
use crate::locale::NoEcc;
use crate::locale::NoModules;

const NO_DRIVER: &str = "核心未載入 IPMI 驅動程式";

impl HostWords for TraditionalChinese {
    fn host_label(&self, label: HostLabel) -> &'static str {
        match label {
            HostLabel::Hardware => "硬體平台",
            HostLabel::Machine => "整機型號",
            HostLabel::Board => "主機板",
            HostLabel::Bios => "BIOS",
            HostLabel::Firmware => "韌體",
            HostLabel::Bmc => "BMC",
            HostLabel::Cpu => "處理器",
            HostLabel::CpuModel => "CPU 型號",
            HostLabel::CpuCores => "CPU 核心",
            HostLabel::CpuClock => "CPU 時脈",
            HostLabel::CpuCaches => "CPU 快取",
            HostLabel::Aes => "AES 指令集",
            HostLabel::Virtualization => "硬體虛擬化",
            HostLabel::Memory => "記憶體",
            HostLabel::MemoryTotal => "記憶體總量",
            HostLabel::MemoryUsed => "已用記憶體",
            HostLabel::MemoryUsedEstimated => "已用記憶體（估算）",
            HostLabel::Swap => "交換空間",
            HostLabel::SwapTotal => "Swap 總量",
            HostLabel::SwapUsed => "已用 Swap",
            HostLabel::Ecc => "ECC 校驗",
            HostLabel::Modules => "記憶體插槽",
            HostLabel::Raid => "RAID 控制器",
            HostLabel::Disks => "實體磁碟",
            HostLabel::Filesystems => "檔案系統",
            HostLabel::Cards => "網路介面",
            HostLabel::Os => "作業系統",
            HostLabel::Kernel => "核心版本",
            HostLabel::Arch => "系統架構",
            HostLabel::Uptime => "執行時間",
            HostLabel::Load => "系統負載",
            HostLabel::Tcp => "TCP 壅塞控制",
        }
    }

    fn host_header(&self, header: HostHeader) -> &'static str {
        match header {
            HostHeader::Device => "裝置",
            HostHeader::Interface => "介面",
            HostHeader::Size => "容量",
            HostHeader::Medium => "媒體",
            HostHeader::Model => "型號",
            HostHeader::Use => "用途",
            HostHeader::MountPoint => "掛載點",
            HostHeader::Type => "類型",
            HostHeader::Used => "已用",
            HostHeader::Available => "可用",
            HostHeader::OnDisks => "所屬磁碟",
            HostHeader::Slot => "插槽",
            HostHeader::Speed => "速率",
            HostHeader::Vendor => "製造商",
            HostHeader::Driver => "驅動程式",
            HostHeader::Link => "連線狀態",
            HostHeader::Mtu => "MTU",
        }
    }

    fn platform(&self, platform: Platform) -> String {
        match platform {
            Platform::Dedicated => "未偵測到虛擬化（獨立實體機）".to_owned(),
            Platform::Vm(Hypervisor::Qemu) => "QEMU 虛擬機器（純軟體模擬）".to_owned(),
            Platform::Vm(hypervisor) => match hypervisor.name() {
                Some(name) => format!("{name} 虛擬機器"),
                None => "虛擬機器（類型未知）".to_owned(),
            },
            Platform::Container(ContainerKind::Wsl) => "WSL".to_owned(),
            Platform::Container(container) => match container.name() {
                Some(name) => format!("{name} 容器"),
                None => "容器（類型未知）".to_owned(),
            },
        }
    }

    fn platform_kind(&self, kind: PlatformKind) -> &'static str {
        match kind {
            PlatformKind::Dedicated => "獨立實體伺服器",
            PlatformKind::Vm => "虛擬機器",
            PlatformKind::Container => "容器",
        }
    }

    fn read_timed_out(&self) -> String {
        "不可用（讀取逾時）".to_owned()
    }

    fn none(&self) -> &'static str {
        "無"
    }

    fn aes(&self, supported: bool) -> &'static str {
        if supported { "支援" } else { "不支援" }
    }

    fn virtualization(&self, virtualization: Virtualization) -> &'static str {
        match virtualization {
            Virtualization::IntelVtX => "支援 (VT-x)",
            Virtualization::AmdV => "支援 (AMD-V)",
            Virtualization::Absent => "不支援",
            Virtualization::NotApplicable => "不適用",
        }
    }

    fn cpu_model(&self, model: &Model) -> String {
        match model {
            Model::Name(name) => name.clone(),
            Model::UnknownPart { vendor, part } => format!("{vendor}（零件代碼 {part:#05x}）"),
            Model::UnknownVendor {
                implementer,
                part: Some(part),
            } => format!("ARM CPU（廠商代碼 {implementer:#04x}，零件代碼 {part:#05x}）"),
            Model::UnknownVendor {
                implementer,
                part: None,
            } => format!("ARM CPU（廠商代碼 {implementer:#04x}）"),
        }
    }

    fn cores(&self, cores: Cores) -> String {
        match cores {
            Cores::Dedicated {
                threads,
                topology: Some((sockets, cores)),
            } if sockets > 1 => format!("{threads} 執行緒（{sockets} 路 CPU，共 {cores} 核）"),
            Cores::Dedicated {
                threads,
                topology: Some((_, cores)),
            } => format!("{threads} 執行緒（{cores} 核）"),
            Cores::Dedicated {
                threads,
                topology: None,
            } => format!("{threads} 執行緒"),
            Cores::Vm(vcpus) => format!("{vcpus} vCPU"),
            Cores::Container {
                usable,
                quota: Some(quota),
            } => format!("{usable} 核（配額 {}）", decimals(quota)),
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
        format!("Raspberry Pi 開機韌體（建置於 {built}）")
    }

    fn bmc(&self, bmc: Option<&Bmc>) -> String {
        match bmc {
            Some(Bmc::Driver {
                vendor,
                firmware,
                ipmi,
            }) => {
                let vendor = vendor.map(|number| {
                    bmc::vendor(number).map_or_else(|| format!("廠商 {number}"), str::to_owned)
                });
                let parts: Vec<String> = [
                    vendor,
                    firmware.as_ref().map(|fw| format!("韌體 {fw}")),
                    ipmi.as_ref().map(|version| format!("IPMI {version}")),
                ]
                .into_iter()
                .flatten()
                .collect();
                if parts.is_empty() {
                    "已偵測到".to_owned()
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
                        format!("{name} 介面")
                    }),
                ]
                .into_iter()
                .flatten()
                .collect();
                if parts.is_empty() {
                    format!("已偵測到；介面、廠商與韌體不可用：{NO_DRIVER}")
                } else {
                    format!(
                        "已偵測到（{}）；廠商與韌體不可用：{NO_DRIVER}",
                        parts.join("，")
                    )
                }
            }
            Some(Bmc::Present) => {
                format!("已偵測到；介面、廠商與韌體不可用：{NO_DRIVER}")
            }
            None => "未偵測到".to_owned(),
        }
    }

    fn ecc(&self, modes: &[Ecc], counts: Option<(u64, u64)>) -> String {
        let counts = counts.map(|(corrected, uncorrected)| {
            format!("自開機以來已修正 {corrected} 次，未修正 {uncorrected} 次")
        });
        let modes: Vec<&str> = modes
            .iter()
            .map(|mode| match mode {
                Ecc::SingleBit => "支援（單位元除錯）",
                Ecc::MultiBit => "支援（多位元除錯）",
                Ecc::Crc => "支援（CRC）",
                Ecc::Parity => "僅同位檢查（無除錯）",
                Ecc::None => "無",
                Ecc::Unknown => "未知",
            })
            .collect();
        match (modes.is_empty(), counts) {
            (false, Some(counts)) => format!("{}；{counts}", modes.join("、")),
            (false, None) => format!(
                "{}；錯誤計數不可用：核心未載入 EDAC 驅動程式",
                modes.join("、")
            ),
            (true, Some(counts)) => format!("除錯方式不可用；{counts}"),
            (true, None) => "不可用".to_owned(),
        }
    }

    fn no_ecc(&self, why: NoEcc) -> String {
        let smbios = match why {
            NoEcc::NotInSmbios => "SMBIOS 未提供除錯資訊",
            NoEcc::NoSmbios => "系統無 SMBIOS 表",
            NoEcc::SmbiosUnreadable => "無法讀取 SMBIOS",
        };
        format!("不可用：{smbios}，且未載入 EDAC 驅動程式")
    }

    fn no_modules(&self, why: NoModules<'_>) -> String {
        match why {
            NoModules::Board(size) => {
                format!("主機板內建記憶體 {size}（來自裝置樹）；無獨立記憶體模組資訊")
            }
            NoModules::NotInSmbios => "不可用：SMBIOS 未描述記憶體插槽".to_owned(),
            NoModules::NoSmbios => "不可用：無法讀取 SMBIOS".to_owned(),
            NoModules::DevMem(error) => {
                format!("不可用：無法讀取 SMBIOS（/dev/mem：{error}）")
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
        "未啟用"
    }

    fn module_speed(&self, configured: Option<u32>, rated: Option<u32>) -> Option<String> {
        match (configured, rated) {
            (Some(configured), Some(rated)) if configured != rated => {
                Some(format!("{configured} MT/s（標稱 {rated}）"))
            }
            (Some(configured), _) => Some(format!("{configured} MT/s")),
            (None, Some(rated)) => Some(format!("{rated} MT/s（標稱）")),
            (None, None) => None,
        }
    }

    fn disabled_modules(&self, slots: &[String]) -> String {
        format!(
            "{} 安裝了韌體標記為未啟用的記憶體模組（容量回報為 0），總容量未計入",
            slots.join("、")
        )
    }

    fn raid_card(&self, card: &Card) -> String {
        let name = card.name.clone().unwrap_or_else(|| {
            let driver = card.driver.as_ref().map_or_else(
                || "未載入驅動程式".to_owned(),
                |driver| format!("驅動 {driver}"),
            );
            let ids = format!("PCI {:04x}:{:04x}", card.ids.vendor, card.ids.device);
            match card.ids.subsystem {
                Some((vendor, device)) => {
                    format!("{ids}（子系統 {vendor:04x}:{device:04x}，{driver}）")
                }
                None => format!("{ids}（{driver}）"),
            }
        });
        if card.disks.is_empty() {
            return format!("{name}：未連接磁碟");
        }
        let groups: Vec<String> = [
            (Attachment::Array, "陣列"),
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
            return "未掛載".to_owned();
        }
        let parts: Vec<String> = uses
            .iter()
            .map(|item| match item {
                Use::Mount(point) if point == "/" => "系統碟 (/)".to_owned(),
                Use::Mount(point) => point.clone(),
                Use::Swap => "swap".to_owned(),
                Use::ZfsPool(pool) => format!("ZFS 儲存池 {pool}"),
            })
            .collect();
        parts.join(", ")
    }

    fn card_link(&self, link: Link) -> String {
        let Link::Up { speed, duplex } = link else {
            return "未連線".to_owned();
        };
        let Some(speed) = speed else {
            return "未回報速率".to_owned();
        };
        let speed = if speed >= 1000 {
            format!("{} Gbit/s", f64::from(speed) / 1000.0)
        } else {
            format!("{speed} Mbit/s")
        };
        match duplex {
            Some(Duplex::Full) => format!("{speed}，全雙工"),
            Some(Duplex::Half) => format!("{speed}，半雙工"),
            None => speed,
        }
    }

    fn card_uses(&self, card: &NetworkCard) -> String {
        let mut parts = Vec::new();
        if let Some(master) = &card.master {
            parts.push(match master.kind {
                MasterKind::Bridge => format!("橋接 {}", master.name),
                MasterKind::Bond => format!("聚合 {}", master.name),
                MasterKind::Other => format!("從屬 {}", master.name),
            });
        }
        match (card.default_route.ipv4, card.default_route.ipv6) {
            (true, true) => parts.push("預設路由 (IPv4, IPv6)".to_owned()),
            (true, false) => parts.push("預設路由 (IPv4)".to_owned()),
            (false, true) => parts.push("預設路由 (IPv6)".to_owned()),
            (false, false) => {}
        }
        if card.virtual_functions > 0 {
            parts.push(format!("{} 個虛擬功能 (VF)", card.virtual_functions));
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
            Bus::Usb => "USB 網路卡".to_owned(),
            Bus::Platform(None) => "主機板內建網路卡".to_owned(),
            Bus::Other(name) if name.is_empty() => "—".to_owned(),
            Bus::Platform(Some(name)) | Bus::Other(name) => name.clone(),
        }
    }

    fn no_cards(&self, container: bool) -> &'static str {
        if container {
            "容器內不可見實體網路卡"
        } else {
            "未偵測到網路卡"
        }
    }

    fn slow_pcie(&self, interface: &str, pcie: Pcie) -> String {
        let link = |link: PcieLink| format!("PCIe {}.0 x{}", link.generation, link.lanes);
        format!(
            "網路卡 {interface} 目前協商於 {}，低於其最高支援的 {}",
            link(pcie.current),
            link(pcie.best)
        )
    }

    fn test_directory(&self) -> &'static str {
        "（測試目錄）"
    }

    fn of_the_host(&self) -> &'static str {
        "（宿主機）"
    }

    fn uptime(&self, uptime: Duration) -> String {
        let minutes = usize::try_from(uptime.as_secs() / 60).unwrap_or(usize::MAX);
        let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
        if days > 0 {
            format!("{days} 天 {hours} 小時 {minutes} 分鐘")
        } else if hours > 0 {
            format!("{hours} 小時 {minutes} 分鐘")
        } else {
            format!("{minutes} 分鐘")
        }
    }

    fn tcp(&self, algorithm: &str, qdisc: Option<&str>) -> String {
        match qdisc {
            Some(qdisc) => format!("{algorithm}（佇列 {qdisc}）"),
            None => algorithm.to_owned(),
        }
    }

    fn reachable(&self) -> String {
        "可用".to_owned()
    }

    fn public_address(&self, address: &str, source: &str) -> String {
        format!("可用；公網位址 {address}（來自 {source}）")
    }

    fn no_public_address(&self, reasons: &[String]) -> String {
        if reasons.is_empty() {
            return "可用；未能取得公網位址".to_owned();
        }
        format!("可用；未能取得公網位址：{}", reasons.join("，"))
    }

    fn no_route(&self, family: &str) -> String {
        format!("目前主機無 {family} 路由")
    }

    fn reach(&self, ip: IpAddr, reach: Reach) -> String {
        match reach {
            Reach::Connected => format!("已連線 {ip}"),
            Reach::TimedOut => format!("連線 {ip} 逾時"),
            Reach::Refused => format!("{ip} 拒絕連線"),
            Reach::NoRoute => format!("無到達 {ip} 的路由"),
            Reach::Failed => format!("無法連線 {ip}"),
        }
    }

    fn why(&self, source: &str, why: Why, family: &str) -> String {
        match why {
            Why::TimedOut => format!("{source} 請求逾時"),
            Why::Status(code) => format!("{source} 回傳 HTTP {code}"),
            Why::NotAnAddress => format!("{source} 未回傳有效的 {family} 位址"),
            Why::NotPublic => format!("{source} 未回傳公網位址"),
            Why::NoAddress => format!("{source} 無 {family} 解析位址"),
            Why::Unreachable => format!("無法連線 {source}"),
        }
    }
}

fn decimals(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
