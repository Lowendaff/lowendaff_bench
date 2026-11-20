//! The network test items: network speed (`network.iperf3`), the TCP throughput and latency
//! between the host and public iperf3 servers, as YABS measures them; China carrier speed
//! (`network.taier`), multi-stream download/upload throughput and `64 B` / `1400 B` packet-loss
//! probes against CAICT GlobalSpeed carrier nodes across mainland China; route tracing
//! (`network.trace`), the route from the host to targets in China and elsewhere and to the
//! nearest YouTube and Netflix cache servers, every hop of it; and BGP paths (`network.bgp`), the
//! globally observed `AS_PATH` topology, direct upstreams and RPKI state of the host's public
//! prefixes.

pub mod bgp;
pub mod iperf3;
pub mod taier;
pub mod trace;
