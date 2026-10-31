//! Connects a `RemoteClient` to a running engine and prints, once a second,
//! what arrived: frames and bytes per message kind, and the show snapshots'
//! size and rate. For checking the server and measuring its traffic.
//!
//! ```sh
//! cargo run -p blaulicht-engine --example remote_probe -- 127.0.0.1:1234 60
//! ```

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use blaulicht_engine::{
    client::{EngineClient, RemoteClient},
    server::protocol::Stream,
};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let addr = args.next().unwrap_or_else(|| "127.0.0.1:1234".to_string());
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);

    let client = RemoteClient::connect(addr.as_str(), &Stream::ALL)?;
    println!(
        "connected to {addr} as {} in {} (engine {})",
        client.client(),
        client.session(),
        client.engine_version()
    );
    if client.wait_for_snapshot(Duration::from_secs(10)) {
        let (groups, scenes) = client.with_show(|show| (show.0.groups.len(), show.0.scenes.len()));
        println!("first snapshot: {groups} groups, {scenes} scenes");
    } else {
        println!("no snapshot within 10 s");
    }

    let start = Instant::now();
    let mut last = client.stats().by_kind;
    let mut shows = Vec::new();
    for second in 1..=seconds {
        std::thread::sleep(Duration::from_secs(1));
        // Read what the frontend would, so nothing piles up.
        while client.try_recv_system().is_some() {}
        while client.try_recv_bus().is_some() {}
        client.take_results();

        let stats = client.stats();
        let mut line = format!("{second:>3}s");
        for (kind, (frames, bytes)) in &stats.by_kind {
            let (old_frames, old_bytes) = last.get(kind).copied().unwrap_or_default();
            let (frames, bytes) = (frames - old_frames, bytes - old_bytes);
            if frames > 0 {
                line += &format!("  {kind} {frames}/s {:.1} KB/s", bytes as f64 / 1024.0);
            }
            if *kind == "Show" && frames > 0 {
                shows.push((frames, bytes));
            }
        }
        line += &format!(
            "  (last show {:.1} KB)",
            stats.last_show_bytes as f64 / 1024.0
        );
        println!("{line}");
        last = stats.by_kind;
        if !client.is_connected() {
            println!("disconnected");
            break;
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let totals: BTreeMap<_, _> = client.stats().by_kind;
    println!("totals over {elapsed:.1} s:");
    for (kind, (frames, bytes)) in totals {
        println!(
            "  {kind:<18} {frames:>7} frames  {:>8.2} frames/s  {:>10.1} KB/s  {:>9.1} KB/frame",
            frames as f64 / elapsed,
            bytes as f64 / 1024.0 / elapsed,
            bytes as f64 / 1024.0 / frames.max(1) as f64
        );
    }
    Ok(())
}
