//! Network cards: the captured hosts' cards, and synthetic hosts laid out as sysfs is, with
//! relative links, for the cases that no captured host has.

mod common;

use bench_host::Root;
use bench_host::cards;
use bench_host::cards::Bus;
use bench_host::cards::Duplex;
use bench_host::cards::Link;
use bench_host::cards::Master;
use bench_host::cards::MasterKind;
use bench_host::cards::NetworkCard;
use bench_host::cards::Pcie;
use bench_host::cards::PcieLink;
use bench_host::cards::Routes;
use tempfile::TempDir;

/// The parent of every synthetic device.
const BUS: &str = "/sys/devices/pci0000:00";

/// A synthetic host with these default routes.
fn host(ipv4: &[&str], ipv6: &[&str]) -> TempDir {
    let dir = common::host((&[], &[]));
    let mut route =
        "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n"
            .to_owned();
    for interface in ipv4 {
        route.push_str(&format!(
            "{interface}\t00000000\t00000000\t0003\t0\t0\t0\t00000000\t0\t0\t0\n"
        ));
    }
    common::add_bytes(&dir, "/proc/net/route", route.as_bytes());
    let zero = "0".repeat(32);
    let mut route6 = String::new();
    for interface in ipv6 {
        route6.push_str(&format!(
            "{zero} 00 {zero} 00 {zero} 00000400 00000001 00000000 00000003 {interface}\n"
        ));
    }
    common::add_bytes(&dir, "/proc/net/ipv6_route", route6.as_bytes());
    dir
}

/// An interface: its class link, and at its own path these files; with a device (its PCI
/// address under [`BUS`]) its `device` link, as sysfs links them.
fn interface(dir: &TempDir, name: &str, device: Option<&str>, files: &[(&str, &str)]) {
    let own = match device {
        Some(address) => format!("{BUS}/{address}/net/{name}"),
        None => format!("/sys/devices/virtual/net/{name}"),
    };
    common::link(
        dir,
        &format!("/sys/class/net/{name}"),
        &format!("../..{}", own.trim_start_matches("/sys")),
    );
    for (file, contents) in files {
        common::add_bytes(
            dir,
            &format!("{own}/{file}"),
            format!("{contents}\n").as_bytes(),
        );
    }
    if let Some(address) = device {
        common::link(
            dir,
            &format!("{own}/device"),
            &format!("../../../{address}"),
        );
    }
}

/// A device under [`BUS`]: its bus, its driver, and these files.
fn device(dir: &TempDir, address: &str, bus: &str, driver: Option<&str>, files: &[(&str, &str)]) {
    let path = format!("{BUS}/{address}");
    common::link(
        dir,
        &format!("{path}/subsystem"),
        &format!("../../../bus/{bus}"),
    );
    if let Some(driver) = driver {
        common::link(
            dir,
            &format!("{path}/driver"),
            &format!("../../../bus/{bus}/drivers/{driver}"),
        );
    }
    for (file, contents) in files {
        common::add_bytes(
            dir,
            &format!("{path}/{file}"),
            format!("{contents}\n").as_bytes(),
        );
    }
}

/// `name`'s bridge or bond, as the link at the interface's own path that sysfs names `master`.
fn master(dir: &TempDir, name: &str, master: &str) {
    let class = Root::new(dir.path())
        .link(&format!("/sys/class/net/{name}"))
        .expect("the interface's class link");
    let own = format!("/sys/{}", class.trim_start_matches("../../"));
    common::link(
        dir,
        &format!("{own}/master"),
        &format!("../../../virtual/net/{master}"),
    );
}

/// A card's interface, link, bridge or bond, and default routes.
fn summary(card: &NetworkCard) -> (&str, Link, Option<&Master>, Routes) {
    (
        card.interface.as_str(),
        card.link,
        card.master.as_ref(),
        card.default_route,
    )
}

const UP_FULL: [(&str, &str); 4] = [
    ("carrier", "1"),
    ("operstate", "up"),
    ("speed", "1000"),
    ("duplex", "full"),
];

fn up(speed: Option<u32>, duplex: Option<Duplex>) -> Link {
    Link::Up { speed, duplex }
}

const IPV4: Routes = Routes {
    ipv4: true,
    ipv6: false,
};

const NONE: Routes = Routes {
    ipv4: false,
    ipv6: false,
};

#[test]
fn the_vm_has_one_virtio_card_whose_speed_its_driver_does_not_report() {
    let cards = cards::read(&common::fixture("kvm-debian13-x86_64"));
    assert_eq!(
        cards,
        [NetworkCard {
            interface: "eth0".to_owned(),
            bus: Bus::Pci {
                vendor: 0x1af4,
                device: 0x1000,
            },
            model: Some("Red Hat Virtio network device".to_owned()),
            driver: Some("virtio_net".to_owned()),
            link: up(None, None),
            mtu: Some(1500),
            pcie: None,
            virtual_functions: 0,
            master: None,
            default_route: IPV4,
        }]
    );
}

#[test]
fn the_servers_port_in_the_bridge_carries_the_default_route_at_one_gigabit() {
    let cards = cards::read(&common::fixture("dedicated-pve-x86_64"));
    let bridge = Master {
        name: "vmbr0".to_owned(),
        kind: MasterKind::Bridge,
    };
    let summaries: Vec<_> = cards.iter().map(summary).collect();
    assert_eq!(
        summaries,
        [
            ("ens33f0", Link::Down, None, NONE),
            (
                "ens33f1",
                up(Some(1000), Some(Duplex::Full)),
                Some(&bridge),
                IPV4
            ),
            ("ens51f0np0", Link::Down, None, NONE),
            ("ens51f1np1", Link::Down, None, NONE),
        ]
    );
    let pcie = |generation, lanes| PcieLink { generation, lanes };
    let models: Vec<_> = cards
        .iter()
        .map(|card| (card.model.as_deref(), card.driver.as_deref(), card.pcie))
        .collect();
    let intel = (
        Some("Intel Ethernet Controller 10-Gigabit X540-AT2"),
        Some("ixgbe"),
        Some(Pcie {
            current: pcie(2, 8),
            best: pcie(2, 8),
        }),
    );
    let mellanox = (
        Some("Mellanox MT27710 Family [ConnectX-4 Lx]"),
        Some("mlx5_core"),
        Some(Pcie {
            current: pcie(3, 8),
            best: pcie(3, 8),
        }),
    );
    assert_eq!(models, [intel, intel, mellanox, mellanox]);
    assert!(
        cards
            .iter()
            .all(|card| !card.pcie.is_some_and(|pcie| pcie.degraded()))
    );
}

#[test]
fn the_boards_own_ethernet_goes_by_its_device_tree_name() {
    let cards = cards::read(&common::fixture("rock3a-debian13-aarch64"));
    assert_eq!(cards.len(), 1, "{cards:?}");
    let card = &cards[0];
    assert_eq!(
        card.bus,
        Bus::Platform(Some("rockchip,rk3568-gmac".to_owned()))
    );
    assert_eq!(card.model, None);
    assert_eq!(card.driver.as_deref(), Some("rk_gmac-dwmac"));
    assert_eq!(
        summary(card),
        ("end0", up(Some(1000), Some(Duplex::Full)), None, IPV4)
    );
}

#[test]
fn the_fourth_hosts_built_in_controller_runs_one_port_at_ten_gigabit() {
    let cards = cards::read(&common::fixture("dedicated-ubuntu24-x86_64"));
    let summaries: Vec<_> = cards.iter().map(summary).collect();
    assert_eq!(
        summaries,
        [
            ("enp61s0f1np1", Link::Down, None, NONE),
            ("eth0", up(Some(10_000), Some(Duplex::Full)), None, IPV4),
        ]
    );
    for card in &cards {
        assert_eq!(
            card.model.as_deref(),
            Some("Intel Ethernet Connection X722 for 10GbE SFP+")
        );
        // Built into the chipset, the controller reports its PCIe link as 2.5 GT/s x1 of the
        // same, which is not slower than it can.
        assert!(card.pcie.is_some_and(|pcie| !pcie.degraded()), "{card:?}");
    }
}

#[test]
fn a_container_sees_no_card() {
    for name in ["docker-debian13-aarch64", "lxc-pve-debian13-x86_64"] {
        assert_eq!(cards::read(&common::fixture(name)), [], "{name}");
    }
}

#[test]
fn interfaces_without_a_device_are_no_cards() {
    let dir = host(&["vmbr0"], &[]);
    interface(&dir, "lo", None, &[("carrier", "1")]);
    interface(&dir, "vmbr0", None, &UP_FULL);
    common::add_bytes(
        &dir,
        "/sys/devices/virtual/net/vmbr0/bridge/stp_state",
        b"0\n",
    );
    interface(&dir, "veth101i0", None, &UP_FULL);
    common::add_bytes(&dir, "/sys/class/net/bonding_masters", b"bond0\n");
    assert_eq!(cards::read(&Root::new(dir.path())), []);
}

#[test]
fn a_bond_in_a_bridge_carries_the_bridges_default_route_to_both_of_its_cards() {
    let dir = host(&["vmbr0"], &["vmbr0"]);
    for (name, address) in [("eno1", "0000:01:00.0"), ("eno2", "0000:01:00.1")] {
        device(
            &dir,
            address,
            "pci",
            Some("igb"),
            &[("vendor", "0x8086"), ("device", "0x1521")],
        );
        interface(&dir, name, Some(address), &UP_FULL);
        master(&dir, name, "bond0");
    }
    interface(&dir, "bond0", None, &UP_FULL);
    common::add_bytes(
        &dir,
        "/sys/devices/virtual/net/bond0/bonding/mode",
        b"802.3ad 4\n",
    );
    master(&dir, "bond0", "vmbr0");
    interface(&dir, "vmbr0", None, &UP_FULL);
    common::add_bytes(
        &dir,
        "/sys/devices/virtual/net/vmbr0/bridge/stp_state",
        b"0\n",
    );
    let cards = cards::read(&Root::new(dir.path()));
    let bond = Master {
        name: "bond0".to_owned(),
        kind: MasterKind::Bond,
    };
    let both = Routes {
        ipv4: true,
        ipv6: true,
    };
    let full = up(Some(1000), Some(Duplex::Full));
    let summaries: Vec<_> = cards.iter().map(summary).collect();
    assert_eq!(
        summaries,
        [
            ("eno1", full, Some(&bond), both),
            ("eno2", full, Some(&bond), both)
        ]
    );
    assert_eq!(
        cards[0].bus,
        Bus::Pci {
            vendor: 0x8086,
            device: 0x1521,
        }
    );
}

#[test]
fn a_master_that_is_neither_bridge_nor_bond_is_named_as_it_is() {
    let dir = host(&[], &[]);
    device(&dir, "0000:02:00.0", "pci", None, &[]);
    interface(&dir, "ens1", Some("0000:02:00.0"), &UP_FULL);
    master(&dir, "ens1", "team0");
    interface(&dir, "team0", None, &UP_FULL);
    let cards = cards::read(&Root::new(dir.path()));
    assert_eq!(
        cards[0].master,
        Some(Master {
            name: "team0".to_owned(),
            kind: MasterKind::Other,
        })
    );
    // Without readable ids, the bus is named as it is.
    assert_eq!(cards[0].bus, Bus::Other("pci".to_owned()));
}

#[test]
fn a_loop_of_masters_ends() {
    let dir = host(&["b"], &[]);
    device(&dir, "0000:03:00.0", "pci", None, &[]);
    interface(&dir, "a", Some("0000:03:00.0"), &UP_FULL);
    interface(&dir, "b", None, &UP_FULL);
    master(&dir, "a", "b");
    master(&dir, "b", "a");
    let cards = cards::read(&Root::new(dir.path()));
    assert_eq!(cards[0].default_route, IPV4);
}

#[test]
fn virtual_functions_count_on_their_physical_function() {
    let dir = host(&[], &[]);
    let ids = [
        ("vendor", "0x15b3"),
        ("device", "0x1015"),
        ("sriov_numvfs", "2"),
    ];
    device(&dir, "0000:3b:00.0", "pci", Some("mlx5_core"), &ids);
    interface(&dir, "ens1f0", Some("0000:3b:00.0"), &UP_FULL);
    for (name, address) in [("ens1f0v0", "0000:3b:00.2"), ("ens1f0v1", "0000:3b:00.3")] {
        device(
            &dir,
            address,
            "pci",
            Some("mlx5_core"),
            &[("vendor", "0x15b3")],
        );
        common::link(&dir, &format!("{BUS}/{address}/physfn"), "../0000:3b:00.0");
        interface(&dir, name, Some(address), &UP_FULL);
    }
    let cards = cards::read(&Root::new(dir.path()));
    let rows: Vec<_> = cards
        .iter()
        .map(|card| (card.interface.as_str(), card.virtual_functions))
        .collect();
    assert_eq!(rows, [("ens1f0", 2)]);
}

#[test]
fn a_virtual_function_without_its_physical_function_is_the_vms_card() {
    let dir = host(&["eth0"], &[]);
    device(
        &dir,
        "0000:00:05.0",
        "pci",
        Some("mlx5_core"),
        &[("vendor", "0x15b3"), ("device", "0x1016")],
    );
    common::link(
        &dir,
        &format!("{BUS}/0000:00:05.0/physfn"),
        "../0000:3b:00.0",
    );
    interface(&dir, "eth0", Some("0000:00:05.0"), &UP_FULL);
    let cards = cards::read(&Root::new(dir.path()));
    assert_eq!(cards.len(), 1, "{cards:?}");
    assert_eq!(cards[0].default_route, IPV4);
}

#[test]
fn an_infiniband_port_is_a_card_with_its_rate_as_speed() {
    let dir = host(&[], &[]);
    let ids = [("vendor", "0x15b3"), ("device", "0x1017")];
    device(&dir, "0000:5e:00.0", "pci", Some("mlx5_core"), &ids);
    // IPoIB: type 32, the port's rate (4X EDR) as its speed, and a datagram MTU.
    let files = [
        ("type", "32"),
        ("carrier", "1"),
        ("speed", "100000"),
        ("duplex", "full"),
        ("mtu", "2044"),
    ];
    interface(&dir, "ib0", Some("0000:5e:00.0"), &files);
    let cards = cards::read(&Root::new(dir.path()));
    assert_eq!(cards.len(), 1, "{cards:?}");
    assert_eq!(
        summary(&cards[0]),
        ("ib0", up(Some(100_000), Some(Duplex::Full)), None, NONE)
    );
    assert_eq!(
        (cards[0].mtu, cards[0].driver.as_deref()),
        (Some(2044), Some("mlx5_core"))
    );
}

#[test]
fn a_usb_adapter_is_named_by_udev_and_a_card_without_udev_by_its_ids() {
    let dir = host(&[], &[]);
    device(&dir, "usb2", "usb", Some("r8152"), &[]);
    interface(
        &dir,
        "enx1",
        Some("usb2"),
        &[("carrier", "1"), ("ifindex", "4")],
    );
    common::add_bytes(
        &dir,
        "/run/udev/data/n4",
        b"E:ID_NET_DRIVER=r8152\nE:ID_VENDOR_FROM_DATABASE=Realtek Semiconductor Corp.\n\
          E:ID_MODEL_FROM_DATABASE=RTL8153 Gigabit Ethernet Adapter\n",
    );
    device(
        &dir,
        "0000:04:00.0",
        "pci",
        None,
        &[("vendor", "0x10ec"), ("device", "0x8168")],
    );
    interface(
        &dir,
        "enp4s0",
        Some("0000:04:00.0"),
        &[("carrier", "0"), ("ifindex", "5")],
    );
    common::add_bytes(&dir, "/run/udev/data/n5", b"E:ID_NET_DRIVER=r8169\n");
    let cards = cards::read(&Root::new(dir.path()));
    let rows: Vec<_> = cards
        .iter()
        .map(|card| {
            (
                card.interface.as_str(),
                &card.bus,
                card.model.as_deref(),
                card.driver.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                "enp4s0",
                &Bus::Pci {
                    vendor: 0x10ec,
                    device: 0x8168
                },
                None,
                Some("r8169")
            ),
            (
                "enx1",
                &Bus::Usb,
                Some("Realtek RTL8153 Gigabit Ethernet Adapter"),
                Some("r8152")
            ),
        ]
    );
}

#[test]
fn the_link_reads_the_carrier_and_leaves_out_unknown_speeds() {
    let cases: [(&[(&str, &str)], Link); 8] = [
        (&UP_FULL, up(Some(1000), Some(Duplex::Full))),
        (
            &[("carrier", "0"), ("operstate", "up"), ("speed", "1000")],
            Link::Down,
        ),
        (
            &[("carrier", "1"), ("speed", "100"), ("duplex", "half")],
            up(Some(100), Some(Duplex::Half)),
        ),
        // Reading the carrier fails while the interface is down.
        (&[("operstate", "down")], Link::Down),
        (
            &[("carrier", "1"), ("speed", "-1"), ("duplex", "unknown")],
            up(None, None),
        ),
        (&[("carrier", "1"), ("speed", "4294967295")], up(None, None)),
        (
            &[("carrier", "1"), ("speed", "0"), ("duplex", "full")],
            up(None, Some(Duplex::Full)),
        ),
        (
            &[("carrier", "1"), ("speed", "400000")],
            up(Some(400_000), None),
        ),
    ];
    for (files, link) in cases {
        let dir = host(&[], &[]);
        device(&dir, "0000:05:00.0", "pci", None, &[]);
        interface(&dir, "eth0", Some("0000:05:00.0"), files);
        let cards = cards::read(&Root::new(dir.path()));
        assert_eq!(cards[0].link, link, "{files:?}");
    }
}

#[test]
fn routes_to_a_network_are_no_default_route() {
    let dir = host(&[], &[]);
    device(&dir, "0000:08:00.0", "pci", None, &[]);
    interface(&dir, "eth0", Some("0000:08:00.0"), &UP_FULL);
    // 192.168.2.0/24, and 0.0.0.0/1, which OpenVPN adds to override the default route.
    common::add_bytes(
        &dir,
        "/proc/net/route",
        b"Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
          eth0\t0002A8C0\t00000000\t0001\t0\t0\t0\t00FFFFFF\t0\t0\t0\n\
          eth0\t00000000\t00000000\t0001\t0\t0\t0\t00000080\t0\t0\t0\n",
    );
    // fe80::/64, and ::/64, whose prefix of 64 makes it no default route.
    let zero = "0".repeat(32);
    let route6 = format!(
        "fe80{rest} 40 {zero} 00 {zero} 00000100 00000001 00000000 00000001 eth0\n\
         {zero} 40 {zero} 00 {zero} 00000100 00000001 00000000 00000001 eth0\n",
        rest = "0".repeat(28),
    );
    common::add_bytes(&dir, "/proc/net/ipv6_route", route6.as_bytes());
    let cards = cards::read(&Root::new(dir.path()));
    assert_eq!(cards[0].default_route, NONE);
}

#[test]
fn only_an_ipv6_default_route_marks_only_ipv6() {
    let dir = host(&[], &["eth0", "lo"]);
    device(&dir, "0000:06:00.0", "pci", None, &[]);
    interface(&dir, "eth0", Some("0000:06:00.0"), &UP_FULL);
    let cards = cards::read(&Root::new(dir.path()));
    assert_eq!(
        cards[0].default_route,
        Routes {
            ipv4: false,
            ipv6: true,
        }
    );
}

#[test]
fn a_pcie_link_slower_or_narrower_than_the_cards_best_is_degraded() {
    let link = |speed: &str, lanes: &str, best: &str, most: &str| {
        let dir = host(&[], &[]);
        let files = [
            ("current_link_speed", speed),
            ("current_link_width", lanes),
            ("max_link_speed", best),
            ("max_link_width", most),
        ];
        device(&dir, "0000:07:00.0", "pci", None, &files);
        interface(&dir, "eth0", Some("0000:07:00.0"), &UP_FULL);
        cards::read(&Root::new(dir.path()))[0].pcie
    };
    let pcie = |generation, lanes| PcieLink { generation, lanes };
    let narrow = link("8.0 GT/s PCIe", "4", "8.0 GT/s PCIe", "8").expect("a link");
    assert_eq!(narrow.current, pcie(3, 4));
    assert!(narrow.degraded());
    let slow = link("5.0 GT/s PCIe", "8", "16.0 GT/s PCIe", "8").expect("a link");
    assert_eq!((slow.current, slow.best), (pcie(2, 8), pcie(4, 8)));
    assert!(slow.degraded());
    let full = link("32.0 GT/s PCIe", "16", "32.0 GT/s PCIe", "16").expect("a link");
    assert_eq!(full.best, pcie(5, 16));
    assert!(!full.degraded());
    assert_eq!(link("Unknown", "8", "8.0 GT/s PCIe", "8"), None);
    assert_eq!(link("2.5 GT/s PCIe", "0", "2.5 GT/s PCIe", "1"), None);
    let six = link("64.0 GT/s PCIe", "1", "64.0 GT/s PCIe", "1").expect("a link");
    assert_eq!(six.best, pcie(6, 1));
    // Older kernels write a rate without "PCIe", and a whole one without its decimal: "8 GT/s".
    let rates = [
        ("2.5", 1),
        ("5", 2),
        ("8", 3),
        ("16", 4),
        ("32", 5),
        ("64", 6),
    ];
    for (rate, generation) in rates {
        let speed = format!("{rate} GT/s");
        let old = link(&speed, "4", &speed, "4").expect("a link");
        assert_eq!(old.best, pcie(generation, 4), "{speed}");
    }
}

#[test]
fn a_model_takes_the_makers_short_name_without_the_databases_notes() {
    let cases = [
        (
            "Intel Corporation",
            "Ethernet Controller 10-Gigabit X540-AT2 (10G base-T DP EP102Ti3A Adapter)",
            "Intel Ethernet Controller 10-Gigabit X540-AT2",
        ),
        (
            "Mellanox Technologies",
            "MT27710 Family [ConnectX-4 Lx] (ConnectX-4 Lx Stand-up dual-port 10GbE MCX4121A-XCAT)",
            "Mellanox MT27710 Family [ConnectX-4 Lx]",
        ),
        (
            "Red Hat, Inc.",
            "Virtio network device",
            "Red Hat Virtio network device",
        ),
        (
            "Realtek Semiconductor Co., Ltd.",
            "RTL8111/8168/8211/8411 PCI Express Gigabit Ethernet Controller",
            "Realtek RTL8111/8168/8211/8411 PCI Express Gigabit Ethernet Controller",
        ),
        (
            "Broadcom Inc. and subsidiaries",
            "NetXtreme BCM5720 Gigabit Ethernet PCIe",
            "Broadcom NetXtreme BCM5720 Gigabit Ethernet PCIe",
        ),
        (
            "Amazon.com, Inc.",
            "Elastic Network Adapter (ENA)",
            "Amazon.com Elastic Network Adapter (ENA)",
        ),
        (
            "Advanced Micro Devices, Inc. [AMD]",
            "Ethernet Controller",
            "AMD Ethernet Controller",
        ),
        (
            "VMware",
            "VMXNET3 Ethernet Controller",
            "VMware VMXNET3 Ethernet Controller",
        ),
        (
            "Intel Corporation",
            "Intel(R) Ethernet Controller E810",
            "Intel(R) Ethernet Controller E810",
        ),
        (
            "Marvell Technology Group Ltd.",
            "88E8056 PCI-E Gigabit Ethernet Controller",
            "Marvell 88E8056 PCI-E Gigabit Ethernet Controller",
        ),
        // A Raspberry Pi's USB Ethernet; usb.ids adds the maker's former name.
        (
            "Microchip Technology, Inc. (formerly SMSC)",
            "SMSC9512/9514 Fast Ethernet Adapter",
            "Microchip SMSC9512/9514 Fast Ethernet Adapter",
        ),
        ("Corporation", "Card", "Corporation Card"),
        // A company word counts only as a word of its own.
        ("AcmeCo", "Card", "AcmeCo Card"),
    ];
    for (vendor, model, text) in cases {
        assert_eq!(
            cards::model(Some(vendor), Some(model)).as_deref(),
            Some(text),
            "{vendor} / {model}"
        );
    }
    let words = [
        "and subsidiaries",
        "Corporation",
        "Corp.",
        "Corp",
        "Incorporated",
        "Inc.",
        "Inc",
        "Co.",
        "Co",
        "Ltd.",
        "Ltd",
        "Limited",
        "LLC",
        "GmbH",
        "AG",
        "Technologies",
        "Technology",
        "Semiconductor",
        "Communications",
        "Systems",
        "Group",
    ];
    for word in words {
        for separator in [" ", ",", ", "] {
            let vendor = format!("Acme{separator}{word}");
            assert_eq!(
                cards::model(Some(&vendor), Some("Card")).as_deref(),
                Some("Acme Card"),
                "{vendor}"
            );
        }
    }
    assert_eq!(
        cards::model(None, Some("Card (A B)")).as_deref(),
        Some("Card")
    );
    assert_eq!(
        cards::model(None, Some("Card (A (B) C)")).as_deref(),
        Some("Card (A (B) C)")
    );
    assert_eq!(cards::model(Some("Intel Corporation"), None), None);
    assert_eq!(cards::model(Some("Intel Corporation"), Some("")), None);
}
