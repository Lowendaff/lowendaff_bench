//! Pure functions for CAICT GlobalSpeed (`com.cnspeedtest.globalspeed`) authentication: RFC 1321
//! MD5 digest, Luhn-valid synthetic 15-digit Android IMEI generation, and `dovalid` query strings.

const PKG: &str = "com.cnspeedtest.globalspeed";
const APP: &str = "globalspeed";
const RAND_CONST: &str = "12345555";
/// Standard reported client bandwidth (`Mbps`) expected by `dovalid`.
pub const DEFAULT_BANDWIDTH_MBPS: u32 = 200;

const IMEI_TACS: [&str; 10] = [
    "86309606", "86107500", "86135803", "86330801", "86293103", "86824303", "86436303", "86970103",
    "86114003", "86663003",
];

const MD5_SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const MD5_K: [u32; 64] = [
    0xd76a_a478,
    0xe8c7_b756,
    0x2420_70db,
    0xc1bd_ceee,
    0xf57c_0faf,
    0x4787_c62a,
    0xa830_4613,
    0xfd46_9501,
    0x6980_98d8,
    0x8b44_f7af,
    0xffff_5bb1,
    0x895c_d7be,
    0x6b90_1122,
    0xfd98_7193,
    0xa679_438e,
    0x49b4_0821,
    0xf61e_2562,
    0xc040_b340,
    0x265e_5a51,
    0xe9b6_c7aa,
    0xd62f_105d,
    0x0244_1453,
    0xd8a1_e681,
    0xe7d3_fbc8,
    0x21e1_cde6,
    0xc337_07d6,
    0xf4d5_0d87,
    0x455a_14ed,
    0xa9e3_e905,
    0xfcef_a3f8,
    0x676f_02d9,
    0x8d2a_4c8a,
    0xfffa_3942,
    0x8771_f681,
    0x6d9d_6122,
    0xfde5_380c,
    0xa4be_ea44,
    0x4bde_cfa9,
    0xf6bb_4b60,
    0xbebf_bc70,
    0x289b_7ec6,
    0xeaa1_27fa,
    0xd4ef_3085,
    0x0488_1d05,
    0xd9d4_d039,
    0xe6db_99e5,
    0x1fa2_7cf8,
    0xc4ac_5665,
    0xf429_2244,
    0x432a_ff97,
    0xab94_23a7,
    0xfc93_a039,
    0x655b_59c3,
    0x8f0c_cc92,
    0xffef_f47d,
    0x8584_5dd1,
    0x6fa8_7e4f,
    0xfe2c_e6e0,
    0xa301_4314,
    0x4e08_11a1,
    0xf753_7e82,
    0xbd3a_f235,
    0x2ad7_d2bb,
    0xeb86_d391,
];

/// Computes the lowercase 32-character hexadecimal RFC 1321 MD5 digest of `input`.
#[must_use]
pub fn md5_hex(input: &[u8]) -> String {
    let mut msg = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0: u32 = 0x6745_2301;
    let mut b0: u32 = 0xefcd_ab89;
    let mut c0: u32 = 0x98ba_dcfe;
    let mut d0: u32 = 0x1032_5476;

    let (blocks, _) = msg.as_chunks::<64>();
    for chunk in blocks {
        let mut m = [0_u32; 16];
        for (idx, word) in m.iter_mut().enumerate() {
            let off = idx * 4;
            *word =
                u32::from_le_bytes([chunk[off], chunk[off + 1], chunk[off + 2], chunk[off + 3]]);
        }
        let mut a = a0;
        let mut b = b0;
        let mut c = c0;
        let mut d = d0;
        for i in 0..64 {
            let (f, g) = if i < 16 {
                ((b & c) | ((!b) & d), i)
            } else if i < 32 {
                ((d & b) | ((!d) & c), (5 * i + 1) % 16)
            } else if i < 48 {
                (b ^ c ^ d, (3 * i + 5) % 16)
            } else {
                (c ^ (b | (!d)), (7 * i) % 16)
            };
            let temp = d;
            d = c;
            c = b;
            let sum = a.wrapping_add(f).wrapping_add(MD5_K[i]).wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(MD5_SHIFTS[i]));
            a = temp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }

    let mut digest = [0_u8; 16];
    digest[0..4].copy_from_slice(&a0.to_le_bytes());
    digest[4..8].copy_from_slice(&b0.to_le_bytes());
    digest[8..12].copy_from_slice(&c0.to_le_bytes());
    digest[12..16].copy_from_slice(&d0.to_le_bytes());

    let mut hex = String::with_capacity(32);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Computes the Luhn check digit (`0..=9`) for a numeric prefix (such as a 14-digit IMEI body).
#[must_use]
pub fn luhn_check_digit(digits: &str) -> u8 {
    let mut total: u32 = 0;
    let reversed = digits.bytes().rev().enumerate();
    for (idx, byte) in reversed {
        let mut d = u32::from(byte.saturating_sub(b'0'));
        if idx % 2 == 0 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        total += d;
    }
    ((10 - (total % 10)) % 10) as u8
}

/// Generates a synthetic 15-digit Luhn-valid Android IMEI from `seed`.
#[must_use]
pub fn imei_from_seed(seed: u64) -> String {
    let tac = IMEI_TACS[usize::try_from(seed % (IMEI_TACS.len() as u64)).unwrap_or(0)];
    let serial = (seed / 13) % 1_000_000;
    let body = format!("{tac}{serial:06}");
    let check = luhn_check_digit(&body);
    format!("{body}{check}")
}

/// Computes the CAICT GlobalSpeed `dovalid` token for `(imei, timestamp_secs, bandwidth_mbps)`.
#[must_use]
pub fn make_token(imei: &str, timestamp_secs: u64, bandwidth_mbps: u32) -> String {
    let h1 = md5_hex(format!("model=Android&imei={imei}").as_bytes());
    let h2 = md5_hex(
        format!("stime={timestamp_secs}&band={bandwidth_mbps}&rand={RAND_CONST}").as_bytes(),
    );
    md5_hex(format!("{h1}{h2}").as_bytes())
}

/// Formats the URL query string for `GET /speed/dovalid?...`.
#[must_use]
pub fn dovalid_query(imei: &str, timestamp_secs: u64, bandwidth_mbps: u32, token: &str) -> String {
    format!(
        "key=&flag=true&bandwidth={bandwidth_mbps}&model=Android&imei={imei}\
         &time={timestamp_secs}&app={APP}&token={token}&pkg={PKG}"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn md5_matches_rfc_1321_and_speedtaier_token_vectors() {
        assert_eq!(super::md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(super::md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            super::md5_hex(b"message digest"),
            "f96b697d7cb7938d525a2f31aaf161d0"
        );
        // Exact vector against globalspeed_test.py make_token("863096061234567", "1700000000", 200)
        let imei = super::imei_from_seed(123_456 * 13);
        assert_eq!(imei.len(), 15);
        assert_eq!(super::luhn_check_digit("86309606123456"), 7);
        assert_eq!(
            super::make_token("863096061234567", 1_700_000_000, 200),
            "a92708a28cc42b6edafdffe27d571714"
        );
    }
}
