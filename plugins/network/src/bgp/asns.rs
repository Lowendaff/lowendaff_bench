//! Autonomous System metadata: Tier-1 transit backbones, concise carrier/cloud display names,
//! Tier-1 path truncation, and RIR holder name cleanup.

/// Global Tier-1 transit Autonomous System numbers, sorted ascending for binary search.
pub const TIER1: [u32; 23] = [
    174, 209, 701, 702, 1239, 1273, 1299, 2828, 2914, 3257, 3320, 3356, 3491, 3551, 5511, 6453,
    6461, 6762, 6830, 6939, 7018, 7473, 12956,
];

/// Whether `asn` is one of the global Tier-1 transit backbones.
#[must_use]
pub fn is_tier1(asn: u32) -> bool {
    TIER1.binary_search(&asn).is_ok()
}

/// Truncates an `AS_PATH` (ordered from collector side at index `0` to origin at the end) at the
/// first Tier-1 backbone reached from the origin side, keeping that Tier-1 hop and every
/// downstream hop to the origin while stripping collector-side Tier-1-to-Tier-1 handoffs. When the
/// origin itself is a Tier-1, keeps one upstream Tier-1 peer hop. When no hop is Tier-1, drops
/// only the collector-peer hop when there is more than one ASN.
#[must_use]
pub fn trunc_to_tier1(asns: &[u32]) -> Vec<u32> {
    if asns.len() > 1
        && let Some(idx) = asns[..asns.len() - 1]
            .iter()
            .rposition(|asn| is_tier1(*asn))
    {
        return asns[idx..].to_vec();
    }
    if is_tier1(*asns.last().unwrap_or(&0)) {
        return asns.to_vec();
    }
    if asns.len() > 1 {
        asns[1..].to_vec()
    } else {
        asns.to_vec()
    }
}

/// Built-in concise display names for Tier-1 backbones, major regional carriers, and cloud hosts.
#[must_use]
pub fn known_name(asn: u32) -> Option<&'static str> {
    match asn {
        // Global Tier-1 transit backbones
        174 => Some("Cogent"),
        209 => Some("Lumen Qwest"),
        701 => Some("Verizon"),
        702 => Some("Verizon EMEA"),
        1239 => Some("Sprint"),
        1273 => Some("Vodafone"),
        1299 => Some("Arelion"),
        2828 => Some("XO Verizon"),
        2914 => Some("NTT"),
        3257 => Some("GTT"),
        3320 => Some("DTAG"),
        3356 | 3549 => Some("Lumen"),
        3491 => Some("PCCW"),
        3551 => Some("Savvis"),
        5511 => Some("Orange"),
        6453 => Some("Tata"),
        6461 => Some("Zayo"),
        6762 => Some("Sparkle"),
        6830 => Some("Liberty Global"),
        6939 => Some("Hurricane"),
        7018 => Some("AT&T"),
        7473 => Some("Singtel"),
        12956 => Some("Telxius"),
        // Chinese mainland & HK carriers
        4134 => Some("ChinaNet 163"),
        4538 => Some("CERNET"),
        4808 => Some("CU169 BJ"),
        4809 => Some("ChinaNet CN2"),
        4811 | 4812 => Some("ChinaNet SH"),
        4837 => Some("CU169"),
        9269 => Some("HKBN"),
        9304 => Some("HGC"),
        9808 => Some("CMNET"),
        9929 => Some("CUII 9929"),
        10099 => Some("CUG"),
        17621 => Some("CU Shanghai"),
        17623 => Some("CU Shenzhen"),
        23764 => Some("CTGNet"),
        4515 => Some("PCCW HK"),
        4760 => Some("HKT"),
        56048 => Some("CM Beijing"),
        58453 => Some("CMI"),
        58807 => Some("CMIN2"),
        // Asia-Pacific carriers
        1221 => Some("Telstra"),
        2497 => Some("IIJ"),
        2516 => Some("KDDI"),
        3462 => Some("HiNet"),
        3786 => Some("LG U+"),
        4637 => Some("Telstra Intl"),
        4713 => Some("NTT OCN"),
        4725 => Some("SoftBank"),
        4755 => Some("Tata IN"),
        4766 => Some("KT"),
        4788 => Some("TM Net"),
        7474 => Some("Optus"),
        7552 => Some("Viettel"),
        7713 => Some("Telkom ID"),
        9318 => Some("SK Broadband"),
        9498 => Some("Airtel"),
        17676 => Some("SoftBank BB"),
        45899 => Some("VNPT"),
        55836 => Some("Reliance Jio"),
        // European & American transit / eyeball networks
        286 => Some("KPN"),
        577 => Some("Bell CA"),
        680 => Some("DFN"),
        852 => Some("Telus"),
        3209 => Some("Vodafone DE"),
        3215 => Some("Orange FR"),
        3269 => Some("Telecom IT"),
        5400 => Some("BT"),
        6327 => Some("Shaw"),
        7922 => Some("Comcast"),
        8220 => Some("Colt"),
        8455 => Some("Atom86"),
        9002 => Some("RETN"),
        12322 => Some("Free SAS"),
        20115 => Some("Charter"),
        20764 => Some("RASCOM"),
        22773 => Some("Cox"),
        // Cloud, CDN & VPS hosting networks
        3204 | 3214 | 6233 | 9312 => Some("xTom"),
        3223 => Some("Voxility"),
        8075 => Some("Microsoft"),
        9009 => Some("M247"),
        12876 => Some("Scaleway"),
        13335 => Some("Cloudflare"),
        14061 => Some("DigitalOcean"),
        14618 | 16509 => Some("Amazon"),
        15169 => Some("Google"),
        16265 | 28753 | 60781 | 394380 => Some("Leaseweb"),
        16276 => Some("OVHcloud"),
        19551 => Some("Imperva"),
        20473 => Some("Vultr"),
        20940 => Some("Akamai"),
        21859 => Some("Zenlayer"),
        24940 => Some("Hetzner"),
        31898 => Some("Oracle"),
        36352 => Some("ColoCrossing"),
        37963 | 45102 => Some("Alibaba"),
        41378 => Some("Kirino"),
        45090 | 132203 => Some("Tencent"),
        46562 => Some("Performive"),
        49544 => Some("i3D.net"),
        49981 => Some("WorldStream"),
        51167 => Some("Contabo"),
        51847 => Some("Nearoute"),
        53667 => Some("FranTech"),
        54113 => Some("Fastly"),
        54574 | 133618 => Some("DMIT"),
        59574 | 138997 => Some("Misaka"),
        60068 => Some("Datacamp"),
        61112 => Some("AkileCloud"),
        63150 => Some("BageVM"),
        63949 => Some("Akamai Linode"),
        136907 => Some("Huawei"),
        212238 => Some("CDNext"),
        396982 => Some("Google Cloud"),
        _ => None,
    }
}

/// Cleans a raw RIR `as-overview` `holder` field such as `"CLOUDFLARENET - Cloudflare, Inc."` or
/// `"HETZNER-AS, DE"` into a short display name suitable for compact diagram nodes.
#[must_use]
pub fn clean_holder(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let without_cc = strip_country_suffix(trimmed);
    let candidate = if let Some((left, right)) = without_cc.split_once(" - ") {
        let right_clean = strip_corporate_suffix(right.trim());
        let left_clean = strip_corporate_suffix(left.trim());
        if !right_clean.is_empty() {
            right_clean
        } else {
            left_clean
        }
    } else {
        strip_corporate_suffix(without_cc)
    };
    collapse_spaces(&candidate)
}

fn strip_country_suffix(s: &str) -> &str {
    if s.len() > 4 {
        let bytes = s.as_bytes();
        let n = bytes.len();
        if bytes[n - 4] == b','
            && bytes[n - 3] == b' '
            && bytes[n - 2].is_ascii_uppercase()
            && bytes[n - 1].is_ascii_uppercase()
        {
            return s[..n - 4].trim_end();
        }
    }
    s
}

fn strip_corporate_suffix(s: &str) -> String {
    let mut cur = s.trim();
    for suffix in [
        ", Inc.",
        " Inc.",
        ", Inc",
        " Inc",
        ", LLC.",
        " LLC.",
        ", LLC",
        " LLC",
        ", Ltd.",
        " Ltd.",
        ", Ltd",
        " Ltd",
        " Co., Ltd.",
        " Co., Ltd",
        " Corporation",
        " Corp.",
        " GmbH",
        " S.A.",
        " B.V.",
        " AG",
    ] {
        if let Some(stripped) = cur.strip_suffix(suffix) {
            let trimmed = stripped.trim_end_matches(',').trim();
            if !trimmed.is_empty() {
                cur = trimmed;
                break;
            }
        }
    }
    cur.to_owned()
}

fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}
