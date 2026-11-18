//! The network test items: network speed (`network.iperf3`), the TCP throughput and latency
//! between the host and public iperf3 servers, as YABS measures them; and route tracing
//! (`network.trace`), the route from the host to targets in China and elsewhere and to the
//! nearest YouTube and Netflix cache servers, every hop of it.

pub mod iperf3;
pub mod trace;
