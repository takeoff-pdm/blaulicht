use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::process::{Child, Command as StdCommand, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct Command {
    inner: StdCommand,
}

pub struct CommandHandle {
    child: Child,
}

impl Command {
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Self {
            inner: StdCommand::new(program),
        }
    }

    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.inner.arg(arg);
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.inner.args(args);
        self
    }

    pub fn run(mut self) -> io::Result<CommandHandle> {
        self.inner.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = self.inner.spawn()?;
        Ok(CommandHandle { child })
    }
}

impl CommandHandle {
    pub fn wait(self) -> io::Result<Output> {
        self.child.wait_with_output()
    }

    pub fn wait_timeout(mut self, timeout: Duration) -> io::Result<Output> {
        use std::io::Read;

        // Drain both pipes on background threads: a child writing more than
        // the pipe buffer (~64 KiB) would otherwise block on a full pipe,
        // never exit, and get killed at the timeout with its output lost.
        let mut stdout_pipe = self.child.stdout.take();
        let stdout_thread = thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(ref mut pipe) = stdout_pipe {
                let _ = pipe.read_to_end(&mut buf);
            }
            buf
        });
        let mut stderr_pipe = self.child.stderr.take();
        let stderr_thread = thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(ref mut pipe) = stderr_pipe {
                let _ = pipe.read_to_end(&mut buf);
            }
            buf
        });

        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }

            if Instant::now() >= deadline {
                self.child.kill()?;
                break self.child.wait()?;
            }

            thread::sleep(Duration::from_millis(10));
        };

        Ok(Output {
            status,
            stdout: stdout_thread.join().unwrap_or_default(),
            stderr: stderr_thread.join().unwrap_or_default(),
        })
    }
}

#[derive(Clone)]
pub enum SpawnedCommandState {
    Running,
    Finished(Vec<u8>),
    Failed(Vec<u8>),
}

struct SpawnedCommandEntry {
    plugin_id: u8,
    state: Arc<Mutex<SpawnedCommandState>>,
}

pub struct SpawnedCommandRegistry {
    entries: HashMap<u32, SpawnedCommandEntry>,
    next_handle: u32,
}

impl SpawnedCommandRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            next_handle: 0,
        }
    }

    pub fn insert(&mut self, plugin_id: u8, state: Arc<Mutex<SpawnedCommandState>>) -> u32 {
        self.next_handle = self.next_handle.checked_add(1).unwrap_or(1);
        let handle = self.next_handle;
        self.entries
            .insert(handle, SpawnedCommandEntry { plugin_id, state });
        handle
    }

    pub fn poll(&self, handle: u32) -> Option<SpawnedCommandState> {
        self.entries
            .get(&handle)
            .map(|entry| entry.state.lock().unwrap().clone())
    }

    pub fn remove(&mut self, handle: u32) {
        self.entries.remove(&handle);
    }

    pub fn remove_all_for_plugin(&mut self, plugin_id: u8) {
        self.entries.retain(|_, entry| entry.plugin_id != plugin_id);
    }

    pub fn remove_all(&mut self) {
        self.entries.clear();
    }
}
