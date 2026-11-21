//! Direct DNS `A` lookups (for DNSBL checks) and non-intrusive TCP port-25 SMTP banner probes.
//! Kept in `bench` so plugins never open sockets of their own.

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU16;
use std::sync::atomic::Ordering;
use std::time::Duration;

use bench_api::CancellationToken;
use bench_api::CloudflareStun;
use bench_api::CloudflareStunSpec;
use bench_api::CloudflareTransfer;
use bench_api::CloudflareTransferSpec;
use bench_api::DnsAnswer;
use bench_api::FamilyFilter;
use bench_api::InetspeedTarget;
use bench_api::InetspeedTransfer;
use bench_api::InetspeedTransferSpec;
use bench_api::IpFamily;
use bench_api::NetClient;
use bench_api::NetError;
use bench_api::NetFuture;
use bench_api::SmtpBanner;
use bench_api::TaierAdmission;
use bench_api::TaierStreamSpec;
use bench_api::TaierTransfer;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpSocket;
use tokio::net::TcpStream;
use tokio::net::UdpSocket;

use crate::cloudflare_probe;
use crate::inetspeed_probe;
use crate::locale;
use crate::taier_probe;

/// Default DNS port.
const DNS_PORT: u16 = 53;

/// Most resolvers tried from `/etc/resolv.conf`.
const MAX_RESOLVERS: usize = 3;

/// Maximum bytes read from an SMTP banner line.
const BANNER_LIMIT: usize = 512;

/// Maximum bytes accepted from a TCP DNS reply.
const MAX_DNS_TCP: usize = 16 * 1024;

static NEXT_TX_ID: AtomicU16 = AtomicU16::new(0x4100);

/// Direct DNS `A` and TCP SMTP banner prober.
#[derive(Debug, Default, Clone, Copy)]
pub struct NetProbers;

impl NetProbers {
    /// Creates a new [`NetProbers`].
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Queries `A` records for `qname` against `server` or `/etc/resolv.conf`.
    pub async fn query_a(
        &self,
        qname: &str,
        server: Option<SocketAddr>,
        timeout: Duration,
    ) -> Result<DnsAnswer, NetError> {
        let resolvers = match server {
            Some(addr) => vec![addr],
            None => read_resolv_conf().await?,
        };
        if resolvers.is_empty() {
            return Err(NetError::NoResolver);
        }
        let per_try = (timeout / u32::try_from(resolvers.len()).unwrap_or(1))
            .max(Duration::from_millis(800))
            .min(timeout);
        let mut last_err = NetError::NoResolver;
        for resolver in resolvers {
            let tx_id = NEXT_TX_ID.fetch_add(1, Ordering::Relaxed);
            let query = encode_query(tx_id, qname)?;
            match tokio::time::timeout(per_try, query_resolver(resolver, &query, tx_id)).await {
                Ok(Ok(answer)) => return Ok(answer),
                Ok(Err(err)) => last_err = err,
                Err(_) => last_err = NetError::TimedOut,
            }
        }
        Err(last_err)
    }

    /// Connects to `host:port` over `family`, reads the three-digit SMTP greeting code, sends
    /// `QUIT\r\n`, and closes the socket.
    pub async fn probe_smtp(
        &self,
        family: IpFamily,
        host: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<SmtpBanner, NetError> {
        let ipv6 = family == IpFamily::Ipv6;
        let outcome =
            match tokio::time::timeout(timeout, connect_and_read_banner(host, port, ipv6)).await {
                Ok(result) => result,
                Err(_) => Err(NetError::TimedOut),
            };
        let summary = match &outcome {
            Ok(banner) => format!("SMTP {}", banner.code),
            Err(err) => net_error_summary(err),
        };
        tracing::debug!("{}", locale::words().smtp_probe(host, port, ipv6, &summary));
        outcome
    }
}

impl NetClient for NetProbers {
    fn dns_a<'a>(
        &'a self,
        qname: &'a str,
        server: Option<SocketAddr>,
        timeout: Duration,
    ) -> NetFuture<'a, DnsAnswer> {
        Box::pin(async move { self.query_a(qname, server, timeout).await })
    }

    fn smtp_banner<'a>(
        &'a self,
        family: IpFamily,
        host: &'a str,
        port: u16,
        timeout: Duration,
    ) -> NetFuture<'a, SmtpBanner> {
        Box::pin(async move { self.probe_smtp(family, host, port, timeout).await })
    }

    fn taier_acquire<'a>(
        &'a self,
        ip: Ipv4Addr,
        port: u16,
        query: &'a str,
        timeout: Duration,
    ) -> NetFuture<'a, TaierAdmission> {
        Box::pin(async move { taier_probe::acquire(ip, port, query, timeout).await })
    }

    fn taier_release<'a>(
        &'a self,
        ip: Ipv4Addr,
        port: u16,
        key: &'a str,
        timeout: Duration,
    ) -> NetFuture<'a, ()> {
        Box::pin(async move {
            taier_probe::release(ip, port, key, timeout).await;
            Ok(())
        })
    }

    fn taier_download<'a>(
        &'a self,
        spec: &'a TaierStreamSpec,
        cancel: &'a CancellationToken,
    ) -> NetFuture<'a, TaierTransfer> {
        Box::pin(async move { taier_probe::download(spec, cancel).await })
    }

    fn taier_upload<'a>(
        &'a self,
        spec: &'a TaierStreamSpec,
        cancel: &'a CancellationToken,
    ) -> NetFuture<'a, TaierTransfer> {
        Box::pin(async move { taier_probe::upload(spec, cancel).await })
    }

    fn cloudflare_download<'a>(
        &'a self,
        spec: &'a CloudflareTransferSpec,
        cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CloudflareTransfer> {
        Box::pin(async move { cloudflare_probe::download(spec, cancel).await })
    }

    fn cloudflare_upload<'a>(
        &'a self,
        spec: &'a CloudflareTransferSpec,
        cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CloudflareTransfer> {
        Box::pin(async move { cloudflare_probe::upload(spec, cancel).await })
    }

    fn cloudflare_udp_loss<'a>(
        &'a self,
        spec: &'a CloudflareStunSpec,
        cancel: &'a CancellationToken,
    ) -> NetFuture<'a, CloudflareStun> {
        Box::pin(async move { cloudflare_probe::udp_loss(spec, cancel).await })
    }

    fn inetspeed_resolve<'a>(
        &'a self,
        host: &'a str,
        families: FamilyFilter,
        timeout: Duration,
    ) -> NetFuture<'a, Vec<IpAddr>> {
        Box::pin(async move { inetspeed_probe::resolve_system(host, families, timeout).await })
    }

    fn inetspeed_probe_rtt<'a>(
        &'a self,
        target: &'a InetspeedTarget,
        count: u8,
        timeout: Duration,
        cancel: &'a CancellationToken,
    ) -> NetFuture<'a, Vec<u64>> {
        Box::pin(async move { inetspeed_probe::probe_rtt(target, count, timeout, cancel).await })
    }

    fn inetspeed_transfer<'a>(
        &'a self,
        spec: &'a InetspeedTransferSpec,
        cancel: &'a CancellationToken,
    ) -> NetFuture<'a, InetspeedTransfer> {
        Box::pin(async move { inetspeed_probe::transfer(spec, cancel).await })
    }
}

async fn read_resolv_conf() -> Result<Vec<SocketAddr>, NetError> {
    let text = tokio::fs::read_to_string("/etc/resolv.conf")
        .await
        .map_err(|_| NetError::NoResolver)?;
    Ok(parse_resolv_conf(&text))
}

fn parse_resolv_conf(text: &str) -> Vec<SocketAddr> {
    let mut servers = Vec::new();
    for line in text.lines() {
        let line = line.split(['#', ';']).next().unwrap_or_default().trim();
        let mut parts = line.split_whitespace();
        if parts.next() != Some("nameserver") {
            continue;
        }
        let Some(raw) = parts.next() else {
            continue;
        };
        let cleaned = raw.split('%').next().unwrap_or(raw);
        if let Ok(ip) = cleaned.parse::<IpAddr>() {
            servers.push(SocketAddr::new(ip, DNS_PORT));
            if servers.len() >= MAX_RESOLVERS {
                break;
            }
        }
    }
    servers
}

async fn query_resolver(
    resolver: SocketAddr,
    query: &[u8],
    tx_id: u16,
) -> Result<DnsAnswer, NetError> {
    let bind_addr = if resolver.is_ipv6() {
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)
    } else {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)
    };
    let socket = UdpSocket::bind(bind_addr)
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    socket
        .connect(resolver)
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    socket
        .send(query)
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    let mut buf = [0_u8; 2048];
    let len = socket
        .recv(&mut buf)
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    match decode_answer(&buf[..len], tx_id)? {
        DecodeOutcome::Answer(answer) => Ok(answer),
        DecodeOutcome::Truncated => query_resolver_tcp(resolver, query, tx_id).await,
    }
}

async fn query_resolver_tcp(
    resolver: SocketAddr,
    query: &[u8],
    tx_id: u16,
) -> Result<DnsAnswer, NetError> {
    let mut stream = TcpStream::connect(resolver)
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    let len = u16::try_from(query.len())
        .map_err(|_| NetError::Transport("DNS query too large".to_owned()))?;
    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    stream
        .write_all(query)
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    let mut len_buf = [0_u8; 2];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    let reply_len = usize::from(u16::from_be_bytes(len_buf));
    if reply_len == 0 || reply_len > MAX_DNS_TCP {
        return Err(NetError::Transport("invalid TCP DNS length".to_owned()));
    }
    let mut reply = vec![0_u8; reply_len];
    stream
        .read_exact(&mut reply)
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    match decode_answer(&reply, tx_id)? {
        DecodeOutcome::Answer(answer) => Ok(answer),
        DecodeOutcome::Truncated => Err(NetError::Transport("TCP DNS reply truncated".to_owned())),
    }
}

enum DecodeOutcome {
    Truncated,
    Answer(DnsAnswer),
}

fn encode_query(tx_id: u16, qname: &str) -> Result<Vec<u8>, NetError> {
    let trimmed = qname.trim_end_matches('.');
    if trimmed.is_empty() || trimmed.len() > 253 || !trimmed.is_ascii() {
        return Err(NetError::Transport("invalid DNS query name".to_owned()));
    }
    let mut buf = Vec::with_capacity(trimmed.len() + 18);
    buf.extend_from_slice(&tx_id.to_be_bytes());
    buf.extend_from_slice(&0x0100_u16.to_be_bytes());
    buf.extend_from_slice(&1_u16.to_be_bytes());
    buf.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    for label in trimmed.split('.') {
        let Ok(len) = u8::try_from(label.len()) else {
            return Err(NetError::Transport("DNS label too long".to_owned()));
        };
        if len == 0 || len > 63 {
            return Err(NetError::Transport("invalid DNS label length".to_owned()));
        }
        buf.push(len);
        buf.extend_from_slice(label.as_bytes());
    }
    buf.push(0);
    buf.extend_from_slice(&1_u16.to_be_bytes());
    buf.extend_from_slice(&1_u16.to_be_bytes());
    Ok(buf)
}

fn decode_answer(packet: &[u8], expected_id: u16) -> Result<DecodeOutcome, NetError> {
    if packet.len() < 12 {
        return Err(NetError::Transport(
            "DNS packet shorter than header".to_owned(),
        ));
    }
    let id = u16::from_be_bytes([packet[0], packet[1]]);
    if id != expected_id {
        return Err(NetError::Transport(
            "DNS transaction ID mismatch".to_owned(),
        ));
    }
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    if flags & 0x8000 == 0 {
        return Err(NetError::Transport(
            "DNS packet is not a response".to_owned(),
        ));
    }
    if flags & 0x0200 != 0 {
        return Ok(DecodeOutcome::Truncated);
    }
    let rcode = flags & 0x000f;
    if rcode == 3 {
        return Ok(DecodeOutcome::Answer(DnsAnswer::NotFound));
    }
    if rcode != 0 {
        return Err(NetError::Transport(format!("DNS RCODE {rcode}")));
    }
    let qdcount = usize::from(u16::from_be_bytes([packet[4], packet[5]]));
    let ancount = usize::from(u16::from_be_bytes([packet[6], packet[7]]));
    let mut pos = 12;
    for _ in 0..qdcount {
        pos = skip_name(packet, pos)?;
        if pos + 4 > packet.len() {
            return Err(NetError::Transport("truncated DNS question".to_owned()));
        }
        pos += 4;
    }
    let mut addrs = Vec::new();
    for _ in 0..ancount {
        pos = skip_name(packet, pos)?;
        if pos + 10 > packet.len() {
            return Err(NetError::Transport(
                "truncated DNS resource record".to_owned(),
            ));
        }
        let rtype = u16::from_be_bytes([packet[pos], packet[pos + 1]]);
        let rclass = u16::from_be_bytes([packet[pos + 2], packet[pos + 3]]);
        let rdlen = usize::from(u16::from_be_bytes([packet[pos + 8], packet[pos + 9]]));
        pos += 10;
        if pos + rdlen > packet.len() {
            return Err(NetError::Transport("truncated DNS RDATA".to_owned()));
        }
        if rtype == 1 && rclass == 1 && rdlen == 4 {
            addrs.push(Ipv4Addr::new(
                packet[pos],
                packet[pos + 1],
                packet[pos + 2],
                packet[pos + 3],
            ));
        }
        pos += rdlen;
    }
    let answer = if addrs.is_empty() {
        DnsAnswer::NotFound
    } else {
        DnsAnswer::Addresses(addrs)
    };
    Ok(DecodeOutcome::Answer(answer))
}

fn skip_name(packet: &[u8], mut pos: usize) -> Result<usize, NetError> {
    for _ in 0..128 {
        let Some(&head) = packet.get(pos) else {
            return Err(NetError::Transport("unterminated DNS name".to_owned()));
        };
        if head == 0 {
            return Ok(pos + 1);
        }
        if head & 0xc0 == 0xc0 {
            if pos + 2 > packet.len() {
                return Err(NetError::Transport("truncated DNS pointer".to_owned()));
            }
            return Ok(pos + 2);
        }
        if head & 0xc0 != 0 {
            return Err(NetError::Transport("invalid DNS label header".to_owned()));
        }
        let len = usize::from(head);
        pos += 1 + len;
        if pos > packet.len() {
            return Err(NetError::Transport("truncated DNS label".to_owned()));
        }
    }
    Err(NetError::Transport(
        "DNS name exceeds label limit".to_owned(),
    ))
}

async fn connect_and_read_banner(
    host: &str,
    port: u16,
    ipv6: bool,
) -> Result<SmtpBanner, NetError> {
    let target = resolve_family(host, port, ipv6).await?;
    let socket = if ipv6 {
        let sock = TcpSocket::new_v6().map_err(|err| NetError::Transport(err.to_string()))?;
        sock.bind(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0))
            .map_err(|err| NetError::Transport(err.to_string()))?;
        sock
    } else {
        let sock = TcpSocket::new_v4().map_err(|err| NetError::Transport(err.to_string()))?;
        sock.bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))
            .map_err(|err| NetError::Transport(err.to_string()))?;
        sock
    };
    let mut stream = socket
        .connect(target)
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    let mut buf = Vec::with_capacity(128);
    let mut chunk = [0_u8; 128];
    while buf.len() < BANNER_LIMIT && !buf.contains(&b'\n') {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|err| NetError::Transport(err.to_string()))?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
    }
    let _ = stream.write_all(b"QUIT\r\n").await;
    let _ = stream.shutdown().await;
    parse_smtp_code(&buf)
}

fn parse_smtp_code(bytes: &[u8]) -> Result<SmtpBanner, NetError> {
    let first_line = bytes.split(|&b| b == b'\n').next().unwrap_or_default();
    let trimmed = String::from_utf8_lossy(first_line);
    let prefix = trimmed.trim_start();
    if prefix.len() < 3 || !prefix.as_bytes()[..3].iter().all(u8::is_ascii_digit) {
        return Err(NetError::Transport("invalid SMTP greeting".to_owned()));
    }
    let code: u16 = prefix[..3]
        .parse()
        .map_err(|_| NetError::Transport("invalid SMTP code".to_owned()))?;
    Ok(SmtpBanner { code })
}

async fn resolve_family(host: &str, port: u16, ipv6: bool) -> Result<SocketAddr, NetError> {
    let clean = host.trim_matches(['[', ']']);
    if let Ok(ip) = clean.parse::<IpAddr>() {
        return if ip.is_ipv6() == ipv6 {
            Ok(SocketAddr::new(ip, port))
        } else {
            Err(NetError::NoAddress)
        };
    }
    let addresses = tokio::net::lookup_host((clean, port))
        .await
        .map_err(|err| NetError::Transport(err.to_string()))?;
    for addr in addresses {
        if addr.is_ipv6() == ipv6 {
            return Ok(addr);
        }
    }
    Err(NetError::NoAddress)
}

fn net_error_summary(err: &NetError) -> String {
    match err {
        NetError::NoAddress => "no address for family".to_owned(),
        NetError::TimedOut => "timed out".to_owned(),
        NetError::NoResolver => "no DNS resolver".to_owned(),
        NetError::Transport(why) => why.clone(),
        NetError::Cancelled => "interrupted".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use bench_api::DnsAnswer;
    use bench_api::IpFamily;
    use bench_api::NetError;
    use bench_api::SmtpBanner;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::net::UdpSocket;

    use super::NetProbers;

    #[test]
    fn resolv_conf_extracts_up_to_three_valid_nameservers() {
        let sample = "\
            # comment\n\
            search example.com\n\
            nameserver 1.1.1.1\n\
            nameserver 2606:4700:4700::1111%eth0 ; inline\n\
            nameserver not-an-ip\n\
            nameserver 8.8.8.8\n\
            nameserver 9.9.9.9\n";
        let parsed = super::parse_resolv_conf(sample);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], "1.1.1.1:53".parse().expect("v4"));
        assert_eq!(parsed[1], "[2606:4700:4700::1111]:53".parse().expect("v6"));
        assert_eq!(parsed[2], "8.8.8.8:53".parse().expect("v4"));
    }

    #[tokio::test]
    async fn dns_prober_handles_nxdomain_compressed_answers_and_tcp_fallback() {
        let udp = UdpSocket::bind("127.0.0.1:0").await.expect("bind udp");
        let addr = udp.local_addr().expect("udp addr");
        let tcp = TcpListener::bind(addr).await.expect("bind tcp same port");

        tokio::spawn(async move {
            let mut buf = [0_u8; 512];
            while let Ok((len, peer)) = udp.recv_from(&mut buf).await {
                if len < 12 {
                    continue;
                }
                let id = [buf[0], buf[1]];
                let qtext = String::from_utf8_lossy(&buf[12..len]);
                let mut reply = Vec::new();
                reply.extend_from_slice(&id);
                if qtext.contains("trunc") {
                    // QR=1, TC=1 -> triggers TCP fallback.
                    reply.extend_from_slice(&0x8200_u16.to_be_bytes());
                    reply.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]);
                    reply.extend_from_slice(&buf[12..len]);
                } else if qtext.contains("clean") {
                    // QR=1, RCODE=3 (NXDOMAIN).
                    reply.extend_from_slice(&0x8183_u16.to_be_bytes());
                    reply.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]);
                    reply.extend_from_slice(&buf[12..len]);
                } else {
                    // QR=1, NOERROR, 1 answer using compression pointer 0xc00c -> 127.0.0.2.
                    reply.extend_from_slice(&0x8180_u16.to_be_bytes());
                    reply.extend_from_slice(&[0, 1, 0, 1, 0, 0, 0, 0]);
                    reply.extend_from_slice(&buf[12..len]);
                    reply.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4]);
                    reply.extend_from_slice(&[127, 0, 0, 2]);
                }
                let _ = udp.send_to(&reply, peer).await;
            }
        });

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = tcp.accept().await {
                let mut len_buf = [0_u8; 2];
                if stream.read_exact(&mut len_buf).await.is_err() {
                    continue;
                }
                let qlen = usize::from(u16::from_be_bytes(len_buf));
                let mut qbuf = vec![0_u8; qlen];
                if stream.read_exact(&mut qbuf).await.is_err() {
                    continue;
                }
                let mut reply = Vec::new();
                reply.extend_from_slice(&qbuf[0..2]);
                reply.extend_from_slice(&0x8180_u16.to_be_bytes());
                reply.extend_from_slice(&[0, 1, 0, 1, 0, 0, 0, 0]);
                reply.extend_from_slice(&qbuf[12..]);
                reply.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4]);
                reply.extend_from_slice(&[127, 0, 0, 4]);
                let rlen = u16::try_from(reply.len()).expect("len");
                let _ = stream.write_all(&rlen.to_be_bytes()).await;
                let _ = stream.write_all(&reply).await;
            }
        });

        let probers = NetProbers::new();
        let limit = Duration::from_secs(3);
        assert_eq!(
            probers
                .query_a("1.100.51.198.clean.example", Some(addr), limit)
                .await,
            Ok(DnsAnswer::NotFound)
        );
        assert_eq!(
            probers
                .query_a("2.0.0.127.hit.example", Some(addr), limit)
                .await,
            Ok(DnsAnswer::Addresses(vec![Ipv4Addr::new(127, 0, 0, 2)]))
        );
        assert_eq!(
            probers
                .query_a("4.0.0.127.trunc.example", Some(addr), limit)
                .await,
            Ok(DnsAnswer::Addresses(vec![Ipv4Addr::new(127, 0, 0, 4)]))
        );
    }

    #[tokio::test]
    async fn smtp_prober_reads_greeting_sends_quit_and_checks_family() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind smtp");
        let addr = listener.local_addr().expect("smtp addr");
        let received = Arc::new(Mutex::new(String::new()));
        let rec_clone = Arc::clone(&received);

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let _ = stream.write_all(b"220 mx.example ESMTP ready\r\n").await;
                let mut buf = [0_u8; 64];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                *rec_clone.lock().expect("lock") = String::from_utf8_lossy(&buf[..n]).into_owned();
            }
        });

        let probers = NetProbers::new();
        let limit = Duration::from_secs(3);
        assert_eq!(
            probers
                .probe_smtp(IpFamily::Ipv6, "127.0.0.1", addr.port(), limit)
                .await,
            Err(NetError::NoAddress)
        );
        assert_eq!(
            probers
                .probe_smtp(IpFamily::Ipv4, "127.0.0.1", addr.port(), limit)
                .await,
            Ok(SmtpBanner { code: 220 })
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(received.lock().expect("lock").as_str(), "QUIT\r\n");
    }
}
