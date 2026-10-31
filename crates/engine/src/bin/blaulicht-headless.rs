//! `blaulicht-headless`: the engine without a frontend. It loads the last
//! showfile, runs the plugins and the main loop, and saves on its own
//! (autosave, `Quit`). SIGINT or SIGTERM saves the show and stops; another
//! signal a second or more later stops without waiting for the save.

// Native only, so std::time's clocks are fine here (the engine's clippy.toml
// bans them for code that also runs in a browser).
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::time::{Duration, Instant};

use blaulicht_engine::cli::EngineArgs;
use blaulicht_engine::client::{EngineClient, LocalClient};
use blaulicht_engine::hub::{CommandOutcome, CommandSeq, HostCommand};
use blaulicht_engine::persistence::ExitRequest;
use blaulicht_engine::{Engine, EngineOptions};
use clap::Parser;
use tracing::{error, info, warn};

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// How often the main thread checks for an exit request or a signal.
const POLL: Duration = Duration::from_millis(100);
/// A repeated signal within this time after the first is the same request.
const REPEAT_GRACE: Duration = Duration::from_secs(1);
/// How long a signal waits for the final save before stopping anyway.
const QUIT_TIMEOUT: Duration = Duration::from_secs(15);
/// Exit code when the final save after a signal failed or timed out.
const SAVE_FAILED_CODE: i32 = 1;

/// Runs the Blaulicht engine without a user interface.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct HeadlessArgs {
    #[command(flatten)]
    engine: EngineArgs,
}

/// The `Quit { save: true }` sent after the first signal.
struct PendingQuit {
    client: LocalClient,
    seq: CommandSeq,
    since: Instant,
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::new_heap();

    let args = HeadlessArgs::parse();

    // Before the engine starts: its threads inherit the blocked signals.
    let signals = signals::watch()?;

    let Some(engine) = Engine::start(EngineOptions {
        config_file: args.engine.config_file(),
        attach_desk: false,
    })?
    else {
        return Ok(());
    };
    info!("Running headless; stop with SIGINT or SIGTERM");

    let mut pending: Option<PendingQuit> = None;
    let mut save_failed = false;
    loop {
        // A client's Quit / Restart (or ours, after a signal) ran the
        // shutdown hook.
        if engine.exit_request().is_some() {
            break;
        }
        if let Some(signal) = signals.wait(POLL) {
            if let Some(quit) = &pending {
                // One `kill` of the process group (`timeout`, some service
                // managers) can deliver the signal twice at once.
                if quit.since.elapsed() < REPEAT_GRACE {
                    continue;
                }
                warn!("Received {signal} again; stopping without waiting for the final save");
                save_failed = true;
                break;
            }
            info!("Received {signal}; saving the showfile and stopping");
            let client = engine.connect_client();
            let seq = client.send(HostCommand::Quit { save: true });
            pending = Some(PendingQuit {
                client,
                seq,
                since: Instant::now(),
            });
        }
        if let Some(quit) = &pending {
            let refused = quit.client.take_results().into_iter().find_map(|result| {
                match (result.seq == quit.seq, result.outcome) {
                    (true, CommandOutcome::Refused(why)) => Some(why),
                    _ => None,
                }
            });
            if let Some(why) = refused {
                error!("The final save failed ({why}); stopping anyway");
                save_failed = true;
                break;
            }
            if quit.since.elapsed() > QUIT_TIMEOUT {
                error!("The final save did not finish within {QUIT_TIMEOUT:?}; stopping anyway");
                save_failed = true;
                break;
            }
        }
    }
    drop(pending);

    let exit = engine.shutdown();
    let code = match exit {
        Some(exit) => exit.code(),
        None if save_failed => SAVE_FAILED_CODE,
        None => ExitRequest::Quit.code(),
    };
    info!("Engine stopped");
    std::process::exit(code);
}

#[cfg(unix)]
mod signals {
    use std::time::Duration;

    use crossbeam_channel::{Receiver, RecvTimeoutError};
    use nix::sys::signal::{SigSet, Signal};

    /// SIGINT and SIGTERM, received on a thread of their own.
    pub struct Signals(Receiver<Signal>);

    /// Blocks SIGINT and SIGTERM in this thread and in every thread spawned
    /// after it, and waits for them on a dedicated thread instead (no signal
    /// handler runs). Call it before starting the engine. Child processes
    /// started with `std::process::Command` get a cleared signal mask.
    pub fn watch() -> anyhow::Result<Signals> {
        let mut set = SigSet::empty();
        set.add(Signal::SIGINT);
        set.add(Signal::SIGTERM);
        set.thread_block()?;
        let (sender, receiver) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("signals".to_string())
            .spawn(move || loop {
                match set.wait() {
                    Ok(signal) => {
                        if sender.send(signal).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        tracing::error!("Waiting for signals failed: {error}");
                        break;
                    }
                }
            })?;
        Ok(Signals(receiver))
    }

    impl Signals {
        /// The next signal within `timeout`.
        pub fn wait(&self, timeout: Duration) -> Option<&'static str> {
            match self.0.recv_timeout(timeout) {
                Ok(signal) => Some(signal.as_str()),
                Err(RecvTimeoutError::Timeout) => None,
                // The signal thread failed (logged); keep the poll interval.
                Err(RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(timeout);
                    None
                }
            }
        }
    }
}

#[cfg(not(unix))]
mod signals {
    use std::time::Duration;

    /// No signal handling outside Unix: the process ends on a client's
    /// `Quit` (or is killed).
    pub struct Signals;

    pub fn watch() -> anyhow::Result<Signals> {
        Ok(Signals)
    }

    impl Signals {
        pub fn wait(&self, timeout: Duration) -> Option<&'static str> {
            std::thread::sleep(timeout);
            None
        }
    }
}
