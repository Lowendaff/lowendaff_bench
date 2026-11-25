//! 日本語：システム概要セクションの見出し・ラベル・ハードウェア表記。

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

use super::Japanese;
use crate::locale::Cores;
use crate::locale::HostHeader;
use crate::locale::HostLabel;
use crate::locale::HostWords;
use crate::locale::NoEcc;
use crate::locale::NoModules;

const NO_DRIVER: &str = "カーネルに IPMI ドライバが読み込まれていません";

impl HostWords for Japanese {
    fn host_label(&self, label: HostLabel) -> &'static str {
        match label {
            HostLabel::Hardware => "ハードウェア",
            HostLabel::Machine => "システム型番",
            HostLabel::Board => "マザーボード",
            HostLabel::Bios => "BIOS",
            HostLabel::Firmware => "ファームウェア",
            HostLabel::Bmc => "BMC",
            HostLabel::Cpu => "CPU",
            HostLabel::CpuModel => "CPU 型番",
            HostLabel::CpuCores => "CPU コア構成",
            HostLabel::CpuClock => "CPU クロック",
            HostLabel::CpuCaches => "CPU キャッシュ",
            HostLabel::Aes => "AES 命令",
            HostLabel::Virtualization => "仮想化支援機能",
            HostLabel::Memory => "メモリ",
            HostLabel::MemoryTotal => "メモリ合計",
            HostLabel::MemoryUsed => "メモリ使用量",
            HostLabel::MemoryUsedEstimated => "メモリ使用量(推計)",
            HostLabel::Swap => "Swap",
            HostLabel::SwapTotal => "Swap 合計",
            HostLabel::SwapUsed => "Swap 使用量",
            HostLabel::Ecc => "ECC 訂正",
            HostLabel::Modules => "メモリスロット",
            HostLabel::Raid => "RAID カード",
            HostLabel::Disks => "物理ディスク",
            HostLabel::Filesystems => "ファイルシステム",
            HostLabel::Cards => "ネットワークカード",
            HostLabel::Os => "OS",
            HostLabel::Kernel => "カーネル",
            HostLabel::Arch => "アーキテクチャ",
            HostLabel::Uptime => "稼働時間",
            HostLabel::Load => "ロードアベレージ",
            HostLabel::Tcp => "TCP 輻輳制御",
        }
    }

    fn host_header(&self, header: HostHeader) -> &'static str {
        match header {
            HostHeader::Device => "デバイス",
            HostHeader::Interface => "接続規格",
            HostHeader::Size => "容量",
            HostHeader::Medium => "種別",
            HostHeader::Model => "モデル",
            HostHeader::Use => "用途",
            HostHeader::MountPoint => "マウント先",
            HostHeader::Type => "形式",
            HostHeader::Used => "使用済み",
            HostHeader::Available => "空き容量",
            HostHeader::OnDisks => "構成ディスク",
            HostHeader::Slot => "スロット",
            HostHeader::Speed => "速度",
            HostHeader::Vendor => "メーカー",
            HostHeader::Driver => "ドライバ",
            HostHeader::Link => "リンク状態",
            HostHeader::Mtu => "MTU",
        }
    }

    fn platform(&self, platform: Platform) -> String {
        match platform {
            Platform::Dedicated => "仮想化なし（専用物理サーバー）".to_owned(),
            Platform::Vm(Hypervisor::Qemu) => {
                "QEMU 仮想マシン（ソフトウェアエミュレーション）".to_owned()
            }
            Platform::Vm(hypervisor) => match hypervisor.name() {
                Some(name) => format!("{name} 仮想マシン"),
                None => "仮想マシン（種別不明）".to_owned(),
            },
            Platform::Container(ContainerKind::Wsl) => "WSL".to_owned(),
            Platform::Container(container) => match container.name() {
                Some(name) => format!("{name} コンテナ"),
                None => "コンテナ（種別不明）".to_owned(),
            },
        }
    }

    fn platform_kind(&self, kind: PlatformKind) -> &'static str {
        match kind {
            PlatformKind::Dedicated => "物理サーバー",
            PlatformKind::Vm => "仮想マシン",
            PlatformKind::Container => "コンテナ",
        }
    }

    fn read_timed_out(&self) -> String {
        "取得不可（タイムアウト）".to_owned()
    }

    fn none(&self) -> &'static str {
        "なし"
    }

    fn aes(&self, supported: bool) -> &'static str {
        if supported { "対応" } else { "非対応" }
    }

    fn virtualization(&self, virtualization: Virtualization) -> &'static str {
        match virtualization {
            Virtualization::IntelVtX => "対応 (VT-x)",
            Virtualization::AmdV => "対応 (AMD-V)",
            Virtualization::Absent => "非対応",
            Virtualization::NotApplicable => "対象外",
        }
    }

    fn cpu_model(&self, model: &Model) -> String {
        match model {
            Model::Name(name) => name.clone(),
            Model::UnknownPart { vendor, part } => format!("{vendor}（パーツ番号 {part:#05x}）"),
            Model::UnknownVendor {
                implementer,
                part: Some(part),
            } => format!("ARM CPU（ベンダ {implementer:#04x}、パーツ {part:#05x}）"),
            Model::UnknownVendor {
                implementer,
                part: None,
            } => format!("ARM CPU（ベンダ {implementer:#04x}）"),
        }
    }

    fn cores(&self, cores: Cores) -> String {
        match cores {
            Cores::Dedicated {
                threads,
                topology: Some((sockets, cores)),
            } if sockets > 1 => {
                format!("{threads} スレッド（{sockets} CPU、計 {cores} コア）")
            }
            Cores::Dedicated {
                threads,
                topology: Some((_, cores)),
            } => format!("{threads} スレッド（{cores} コア）"),
            Cores::Dedicated {
                threads,
                topology: None,
            } => format!("{threads} スレッド"),
            Cores::Vm(vcpus) => format!("{vcpus} vCPU"),
            Cores::Container {
                usable,
                quota: Some(quota),
            } => format!("{usable} コア（クォータ {}）", decimals(quota)),
            Cores::Container {
                usable,
                quota: None,
            } => format!("{usable} コア"),
        }
    }

    fn with_detail(&self, name: &str, detail: &str) -> String {
        format!("{name} ({detail})")
    }

    fn u_boot(&self, version: &str) -> String {
        format!("U-Boot {version}")
    }

    fn raspberry_pi(&self, built: &str) -> String {
        format!("Raspberry Pi ブートローダ（ビルド {built}）")
    }

    fn bmc(&self, bmc: Option<&Bmc>) -> String {
        match bmc {
            Some(Bmc::Driver {
                vendor,
                firmware,
                ipmi,
            }) => {
                let vendor = vendor.map(|number| {
                    bmc::vendor(number).map_or_else(|| format!("ベンダ {number}"), str::to_owned)
                });
                let parts: Vec<String> = [
                    vendor,
                    firmware.as_ref().map(|fw| format!("FW {fw}")),
                    ipmi.as_ref().map(|version| format!("IPMI {version}")),
                ]
                .into_iter()
                .flatten()
                .collect();
                if parts.is_empty() {
                    "検出済み".to_owned()
                } else {
                    parts.join("、")
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
                        format!("{name} インターフェース")
                    }),
                ]
                .into_iter()
                .flatten()
                .collect();
                if parts.is_empty() {
                    format!("検出済み；詳細取得不可：{NO_DRIVER}")
                } else {
                    format!(
                        "検出済み（{}）；ベンダ・FW 取得不可：{NO_DRIVER}",
                        parts.join("、")
                    )
                }
            }
            Some(Bmc::Present) => format!("検出済み；詳細取得不可：{NO_DRIVER}"),
            None => "未検出".to_owned(),
        }
    }

    fn ecc(&self, modes: &[Ecc], counts: Option<(u64, u64)>) -> String {
        let counts = counts.map(|(corrected, uncorrected)| {
            format!("起動後 訂正済み {corrected} 件、訂正不能 {uncorrected} 件")
        });
        let modes: Vec<&str> = modes
            .iter()
            .map(|mode| match mode {
                Ecc::SingleBit => "有効（シングルビット訂正）",
                Ecc::MultiBit => "有効（マルチビット訂正）",
                Ecc::Crc => "有効（CRC）",
                Ecc::Parity => "訂正なし（パリティのみ）",
                Ecc::None => "なし",
                Ecc::Unknown => "不明",
            })
            .collect();
        match (modes.is_empty(), counts) {
            (false, Some(counts)) => format!("{}；{counts}", modes.join("、")),
            (false, None) => format!(
                "{}；エラー数取得不可：EDAC ドライバ未読込",
                modes.join("、")
            ),
            (true, Some(counts)) => format!("訂正方式不明；{counts}"),
            (true, None) => "取得不可".to_owned(),
        }
    }

    fn no_ecc(&self, why: NoEcc) -> String {
        let smbios = match why {
            NoEcc::NotInSmbios => "SMBIOS に誤り訂正情報なし",
            NoEcc::NoSmbios => "SMBIOS テーブルなし",
            NoEcc::SmbiosUnreadable => "SMBIOS を読み取れません",
        };
        format!("取得不可：{smbios}（EDAC ドライバも未読込）")
    }

    fn no_modules(&self, why: NoModules<'_>) -> String {
        match why {
            NoModules::Board(size) => {
                format!("オンボードメモリ {size}（デバイスツリー由来）；モジュール情報なし")
            }
            NoModules::NotInSmbios => "取得不可：SMBIOS にモジュール記載なし".to_owned(),
            NoModules::NoSmbios => "取得不可：SMBIOS を読み取れません".to_owned(),
            NoModules::DevMem(error) => {
                format!("取得不可：SMBIOS を読み取れません（/dev/mem：{error}）")
            }
        }
    }

    fn slot(&self, number: usize) -> String {
        format!("スロット {number}")
    }

    fn empty_slot(&self) -> &'static str {
        "空き"
    }

    fn disabled_module(&self) -> &'static str {
        "未使用"
    }

    fn module_speed(&self, configured: Option<u32>, rated: Option<u32>) -> Option<String> {
        match (configured, rated) {
            (Some(configured), Some(rated)) if configured != rated => {
                Some(format!("{configured} MT/s（定格 {rated}）"))
            }
            (Some(configured), _) => Some(format!("{configured} MT/s")),
            (None, Some(rated)) => Some(format!("{rated} MT/s（定格）")),
            (None, None) => None,
        }
    }

    fn disabled_modules(&self, slots: &[String]) -> String {
        format!(
            "{} に未使用（容量 0）として報告されたモジュールがあり、合計から除外しています",
            slots.join("、")
        )
    }

    fn raid_card(&self, card: &Card) -> String {
        let name = card.name.clone().unwrap_or_else(|| {
            let driver = card.driver.as_ref().map_or_else(
                || "ドライバ未読込".to_owned(),
                |driver| format!("ドライバ {driver}"),
            );
            let ids = format!("PCI {:04x}:{:04x}", card.ids.vendor, card.ids.device);
            match card.ids.subsystem {
                Some((vendor, device)) => {
                    format!("{ids}（サブシステム {vendor:04x}:{device:04x}、{driver}）")
                }
                None => format!("{ids}（{driver}）"),
            }
        });
        if card.disks.is_empty() {
            return format!("{name}：接続ディスクなし");
        }
        let groups: Vec<String> = [
            (Attachment::Array, "アレイ"),
            (Attachment::PassThrough, "パススルー"),
            (Attachment::Unknown, "不明"),
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
            None => "不明",
        }
    }

    fn uses(&self, uses: &[Use]) -> String {
        if uses.is_empty() {
            return "未マウント".to_owned();
        }
        let parts: Vec<String> = uses
            .iter()
            .map(|item| match item {
                Use::Mount(point) if point == "/" => "システムディスク (/)".to_owned(),
                Use::Mount(point) => point.clone(),
                Use::Swap => "swap".to_owned(),
                Use::ZfsPool(pool) => format!("ZFS プール {pool}"),
            })
            .collect();
        parts.join(", ")
    }

    fn card_link(&self, link: Link) -> String {
        let Link::Up { speed, duplex } = link else {
            return "リンクダウン".to_owned();
        };
        let Some(speed) = speed else {
            return "速度未報告".to_owned();
        };
        let speed = if speed >= 1000 {
            format!("{} Gbit/s", f64::from(speed) / 1000.0)
        } else {
            format!("{speed} Mbit/s")
        };
        match duplex {
            Some(Duplex::Full) => format!("{speed}、全二重"),
            Some(Duplex::Half) => format!("{speed}、半二重"),
            None => speed,
        }
    }

    fn card_uses(&self, card: &NetworkCard) -> String {
        let mut parts = Vec::new();
        if let Some(master) = &card.master {
            parts.push(match master.kind {
                MasterKind::Bridge => format!("ブリッジ {}", master.name),
                MasterKind::Bond => format!("ボンディング {}", master.name),
                MasterKind::Other => format!("所属 {}", master.name),
            });
        }
        match (card.default_route.ipv4, card.default_route.ipv6) {
            (true, true) => parts.push("既定ルート (IPv4, IPv6)".to_owned()),
            (true, false) => parts.push("既定ルート (IPv4)".to_owned()),
            (false, true) => parts.push("既定ルート (IPv6)".to_owned()),
            (false, false) => {}
        }
        if card.virtual_functions > 0 {
            parts.push(format!("仮想関数 (VF) {} 個", card.virtual_functions));
        }
        if parts.is_empty() {
            "—".to_owned()
        } else {
            parts.join("、")
        }
    }

    fn unnamed_card(&self, bus: &Bus) -> String {
        match bus {
            Bus::Pci { vendor, device } => format!("PCI {vendor:04x}:{device:04x}"),
            Bus::Usb => "USB アダプタ".to_owned(),
            Bus::Platform(None) => "オンボード NIC".to_owned(),
            Bus::Other(name) if name.is_empty() => "—".to_owned(),
            Bus::Platform(Some(name)) | Bus::Other(name) => name.clone(),
        }
    }

    fn no_cards(&self, container: bool) -> &'static str {
        if container {
            "コンテナ内からは物理 NIC 非表示"
        } else {
            "検出されず"
        }
    }

    fn slow_pcie(&self, interface: &str, pcie: Pcie) -> String {
        let link = |link: PcieLink| format!("PCIe {}.0 x{}", link.generation, link.lanes);
        format!(
            "NIC {interface} は {} でリンク中（対応上限 {} より低速）",
            link(pcie.current),
            link(pcie.best)
        )
    }

    fn test_directory(&self) -> &'static str {
        "（測定ディレクトリ）"
    }

    fn of_the_host(&self) -> &'static str {
        "（ホスト側）"
    }

    fn uptime(&self, uptime: Duration) -> String {
        let minutes = usize::try_from(uptime.as_secs() / 60).unwrap_or(usize::MAX);
        let (days, hours, minutes) = (minutes / 1440, minutes / 60 % 24, minutes % 60);
        if days > 0 {
            format!("{days} 日 {hours} 時間 {minutes} 分")
        } else if hours > 0 {
            format!("{hours} 時間 {minutes} 分")
        } else {
            format!("{minutes} 分")
        }
    }

    fn tcp(&self, algorithm: &str, qdisc: Option<&str>) -> String {
        match qdisc {
            Some(qdisc) => format!("{algorithm}（キュー {qdisc}）"),
            None => algorithm.to_owned(),
        }
    }

    fn reachable(&self) -> String {
        "疎通可".to_owned()
    }

    fn public_address(&self, address: &str, source: &str) -> String {
        format!("疎通可；グローバル IP {address}（{source} により確認）")
    }

    fn no_public_address(&self, reasons: &[String]) -> String {
        if reasons.is_empty() {
            return "疎通可；グローバル IP 取得不可".to_owned();
        }
        format!("疎通可；グローバル IP 取得不可：{}", reasons.join("、"))
    }

    fn no_route(&self, family: &str) -> String {
        format!("このホストには {family} ルートがありません")
    }

    fn reach(&self, ip: IpAddr, reach: Reach) -> String {
        match reach {
            Reach::Connected => format!("{ip} に接続成功"),
            Reach::TimedOut => format!("{ip} への接続タイムアウト"),
            Reach::Refused => format!("{ip} が接続を拒否"),
            Reach::NoRoute => format!("{ip} への経路なし"),
            Reach::Failed => format!("{ip} に接続できません"),
        }
    }

    fn why(&self, source: &str, why: Why, family: &str) -> String {
        match why {
            Why::TimedOut => format!("{source} がタイムアウト"),
            Why::Status(code) => format!("{source} が HTTP {code} を返却"),
            Why::NotAnAddress => format!("{source} が有効な {family} アドレスを返さず"),
            Why::NotPublic => format!("{source} がグローバル IP を返さず"),
            Why::NoAddress => format!("{source} に {family} アドレスなし"),
            Why::Unreachable => format!("{source} に到達不可"),
        }
    }
}

fn decimals(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
