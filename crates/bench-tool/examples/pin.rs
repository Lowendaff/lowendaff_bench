//! Prints the pin of a tool for an architecture, one value per line: the URL, the SHA-256 and the
//! size of its download, so that a script can fetch the build that the program pins, as smoke does
//! for its iperf3 server.
//!
//! `cargo run -p bench-tool --example pin -- iperf3 x86_64`

use bench_tool::ALL_TOOLS;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (Some(name), Some(arch), None) = (args.next(), args.next(), args.next()) else {
        return Err("usage: pin <tool> <arch>".to_owned());
    };
    let tool = ALL_TOOLS
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| format!("no tool {name}"))?;
    let asset = tool
        .asset_for(&arch)
        .ok_or_else(|| format!("no build of {name} for {arch}"))?;
    println!("{}\n{}\n{}", asset.url, asset.sha256, asset.size);
    Ok(())
}
