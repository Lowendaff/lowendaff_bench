//! LiTV Taiwan VOD `/api/slur` signed playback probe.

use std::fmt::Write as _;
use std::time;
use std::time::SystemTime;

use bench_api::Context;
use bench_api::HttpRequest;
use bench_api::HttpResponse;
use bench_api::IpFamily;
use sha2::Digest;
use sha2::Sha256;

use super::super::parse;
use super::super::result::Output;
use super::super::result::Verdict;
use super::Platform;
use super::ProbeFuture;

const ASSET_ID: &str = "vod70810-000001M001_1500K";
const MEDIA_TYPE: &str = "vod";

/// Platform specification for LiTV.
pub const SPEC: Platform = Platform {
    id: "litv",
    name: "LiTV",
    probe,
};

/// Probes LiTV over `family` via `generate-device-id`, `PustiService.PUID`, and `/api/slur`.
pub fn probe<'a>(
    ctx: &'a Context<Output>,
    family: IpFamily,
    base_url: Option<&'a str>,
) -> ProbeFuture<'a> {
    Box::pin(async move {
        if let Some(base) = base_url {
            let req =
                HttpRequest::post(family, format!("{base}/{}/check", SPEC.id), b"{}".to_vec())
                    .with_header("content-type", "application/json");
            return match parse::send(ctx, req).await {
                Ok(res) => parse(&res),
                Err(verdict) => verdict,
            };
        }

        let dev_req = HttpRequest::post(
            family,
            "https://www.litv.tv/api/generate-device-id".to_owned(),
            Vec::new(),
        )
        .with_header("content-type", "application/json")
        .with_header("origin", "https://www.litv.tv")
        .with_header("referer", "https://www.litv.tv/");
        let dev_res = match parse::send(ctx, dev_req).await {
            Ok(res) => res,
            Err(verdict) => return verdict,
        };
        if let Some(early) = parse::gate(&dev_res) {
            return early;
        }
        let dev_cookie = parse::cookies_header(&dev_res).unwrap_or_default();

        let puid_req = HttpRequest::post(
            family,
            "https://pusti.svc.litv.tv/puid".to_owned(),
            concat!(
                r#"{"jsonrpc":"2.0","id":100,"method":"PustiService.PUID","#,
                r#""params":{"version":"2.0","device_id":"","device_category":"LTWEB00","#,
                r#""puid":"","aaid":"","idfa":""}}"#
            )
            .as_bytes()
            .to_vec(),
        )
        .with_header("content-type", "application/json")
        .with_header("origin", "https://www.litv.tv")
        .with_header("referer", "https://www.litv.tv/");
        let puid = ctx
            .probe(&puid_req)
            .await
            .ok()
            .and_then(|res| {
                parse::json(&res)?
                    .pointer("/result/puid")?
                    .as_str()
                    .map(str::to_owned)
            })
            .unwrap_or_default();

        let ts_ms = SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .map_or(1_795_620_000_000_u128, |d| d.as_millis());
        let nonce = format!("a1b2c3d4e5f6gh7i8j9k0l1m2n{}", to_base36(ts_ms));
        let sig = sha256_hex(format!("{ASSET_ID}{MEDIA_TYPE}{ts_ms}{nonce}").as_bytes());
        let t_hex = hex_encode(format!("{ASSET_ID}\x1f{MEDIA_TYPE}\x1f{puid}").as_bytes());
        let v_hex = hex_encode(format!("{ts_ms}\x1f{nonce}\x1f{sig}").as_bytes());
        let cookie_hdr = match (dev_cookie.is_empty(), puid.is_empty()) {
            (false, false) => format!("{dev_cookie}; PUID={puid}"),
            (false, true) => dev_cookie,
            (true, false) => format!("PUID={puid}"),
            (true, true) => String::new(),
        };

        let mut slur_req = HttpRequest::post(
            family,
            "https://www.litv.tv/api/slur".to_owned(),
            format!(r#"{{"t":"{t_hex}","v":"{v_hex}"}}"#).into_bytes(),
        )
        .with_header("content-type", "application/json")
        .with_header("origin", "https://www.litv.tv")
        .with_header("referer", "https://www.litv.tv/drama/watch/VOD00328856");
        if !cookie_hdr.is_empty() {
            slur_req = slur_req.with_header("cookie", cookie_hdr);
        }
        match parse::send(ctx, slur_req).await {
            Ok(slur_res) => parse_slur(&slur_res, &sig, &nonce),
            Err(verdict) => verdict,
        }
    })
}

fn parse_slur(response: &HttpResponse, sig: &str, nonce: &str) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if response.status == 200
        && let Some(val) = parse::json(response)
        && let Some(d_hex) = val.get("d").and_then(serde_json::Value::as_str)
        && let Some(plain) = decrypt_slur(d_hex, sig, nonce)
    {
        let unwrapped = HttpResponse {
            status: 200,
            final_url: response.final_url.clone(),
            headers: response.headers.clone(),
            body: plain,
        };
        return parse(&unwrapped);
    }
    parse(response)
}

/// Parses LiTV's decrypted or JSON response into a [`Verdict`].
#[must_use]
pub fn parse(response: &HttpResponse) -> Verdict {
    if let Some(early) = parse::gate(response) {
        return early;
    }
    if matches!(response.status, 403 | 451) {
        return Verdict::no();
    }
    let body = response.text();
    if body.to_ascii_lowercase().contains("outsideregionerror") {
        return Verdict::no();
    }
    if response.status == 200
        && let Some(val) = parse::json(response)
    {
        if let Some(err) = val.get("error").filter(|v| !v.is_null()) {
            let msg = err
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if msg.to_ascii_lowercase().contains("outsideregion") {
                return Verdict::no();
            }
            return Verdict::Unexpected("LiTV RPC error".to_owned());
        }
        let has_path = val
            .get("fullpath")
            .or_else(|| val.pointer("/result/fullpath"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
            || val.pointer("/result/urls").is_some_and(|u| !u.is_null());
        let no_err = val
            .get("errorMessage")
            .is_some_and(serde_json::Value::is_null);
        if has_path || no_err {
            return Verdict::yes_in("TW");
        }
        return Verdict::Unexpected("unrecognized LiTV payload".to_owned());
    }
    Verdict::Unexpected(format!("HTTP {}", response.status))
}

fn to_base36(mut value: u128) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_owned();
    }
    let mut buf = Vec::new();
    while value > 0 {
        buf.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    buf.reverse();
    String::from_utf8(buf).unwrap_or_default()
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn hex_decode(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for &[hi_ch, lo_ch] in hex.as_bytes().as_chunks::<2>().0 {
        let hi = u8::try_from((hi_ch as char).to_digit(16)?).ok()?;
        let lo = u8::try_from((lo_ch as char).to_digit(16)?).ok()?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    hex_encode(&digest)
}

/// Decrypts LiTV's `/api/slur` `iv (12B) || tag (16B) || ciphertext` payload using AES-256-CTR
/// (`J0 + 1 = iv || 0x00000002`) with `key = SHA-256(sig || nonce)`.
#[must_use]
pub fn decrypt_slur(d_hex: &str, sig: &str, nonce: &str) -> Option<Vec<u8>> {
    let raw = hex_decode(d_hex.trim())?;
    if raw.len() <= 28 {
        return None;
    }
    let mut key = [0_u8; 32];
    key.copy_from_slice(&Sha256::digest(format!("{sig}{nonce}").as_bytes()));
    let round_keys = aes256_expand_key(&key);

    let ciphertext = &raw[28..];
    let mut plain = Vec::with_capacity(ciphertext.len());
    let mut counter = 2_u32;
    for chunk in ciphertext.chunks(16) {
        let mut block = [0_u8; 16];
        block[..12].copy_from_slice(&raw[..12]);
        block[12..].copy_from_slice(&counter.to_be_bytes());
        counter = counter.wrapping_add(1);
        let ks = aes256_encrypt_block(block, &round_keys);
        for (c, k) in chunk.iter().zip(ks.iter()) {
            plain.push(c ^ k);
        }
    }
    Some(plain)
}

const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

fn sub_word(word: u32) -> u32 {
    let b = word.to_be_bytes();
    u32::from_be_bytes([
        SBOX[b[0] as usize],
        SBOX[b[1] as usize],
        SBOX[b[2] as usize],
        SBOX[b[3] as usize],
    ])
}

fn aes256_expand_key(key: &[u8; 32]) -> [[u8; 16]; 15] {
    const RCON: [u8; 8] = [0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40];
    let mut w = [0_u32; 60];
    for (i, &chunk) in key.as_chunks::<4>().0.iter().enumerate() {
        w[i] = u32::from_be_bytes(chunk);
    }
    for i in 8..60 {
        let mut temp = w[i - 1];
        if i % 8 == 0 {
            temp = sub_word(temp.rotate_left(8)) ^ (u32::from(RCON[i / 8]) << 24);
        } else if i % 8 == 4 {
            temp = sub_word(temp);
        }
        w[i] = w[i - 8] ^ temp;
    }
    let mut rk = [[0_u8; 16]; 15];
    for (round, slot) in rk.iter_mut().enumerate() {
        for col in 0..4 {
            slot[col * 4..col * 4 + 4].copy_from_slice(&w[round * 4 + col].to_be_bytes());
        }
    }
    rk
}

fn xtime(x: u8) -> u8 {
    (x << 1) ^ if x & 0x80 != 0 { 0x1b } else { 0x00 }
}

fn aes256_encrypt_block(mut s: [u8; 16], rk: &[[u8; 16]; 15]) -> [u8; 16] {
    for (b, k) in s.iter_mut().zip(rk[0].iter()) {
        *b ^= *k;
    }
    let rounds = rk.iter().enumerate().skip(1);
    for (round, round_key) in rounds {
        for b in &mut s {
            *b = SBOX[*b as usize];
        }
        s = [
            s[0], s[5], s[10], s[15], s[4], s[9], s[14], s[3], s[8], s[13], s[2], s[7], s[12],
            s[1], s[6], s[11],
        ];
        if round < 14 {
            for col in 0..4 {
                let idx = col * 4;
                let (a0, a1, a2, a3) = (s[idx], s[idx + 1], s[idx + 2], s[idx + 3]);
                let t = a0 ^ a1 ^ a2 ^ a3;
                s[idx] ^= t ^ xtime(a0 ^ a1);
                s[idx + 1] ^= t ^ xtime(a1 ^ a2);
                s[idx + 2] ^= t ^ xtime(a2 ^ a3);
                s[idx + 3] ^= t ^ xtime(a3 ^ a0);
            }
        }
        for (b, k) in s.iter_mut().zip(round_key.iter()) {
            *b ^= *k;
        }
    }
    s
}
