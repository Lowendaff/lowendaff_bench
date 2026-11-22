//! The network test items: network speed (`network.iperf3`), the TCP throughput and latency
//! between the host and public iperf3 servers, as YABS measures them; China carrier speed
//! (`network.taier`), multi-stream download/upload throughput against CAICT GlobalSpeed carrier
//! nodes across mainland China; Cloudflare speed (`network.cloudflare`), multi-worker HTTPS
//! download/upload throughput, idle/loaded latency, bufferbloat/stability grading, and RFC 5389
//! STUN UDP packet-loss probing against `speed.cloudflare.com`; Apple CDN speed
//! (`network.inetspeed`), Dual DoH endpoint discovery, IP-pinned RTT, and single/multi-thread
//! `mensura.cdn-apple.com` throughput; route tracing (`network.trace`), every hop to targets in
//! China and elsewhere and to the nearest YouTube and Netflix cache servers; and BGP paths
//! (`network.bgp`), the globally observed `AS_PATH` topology, direct upstreams and RPKI state of
//! the host's public prefixes.

#![warn(missing_docs)]

pub mod bgp;
pub mod cloudflare;
pub mod inetspeed;
pub mod iperf3;
pub mod taier;
pub mod trace;
