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
        let deadline = Instant::now() + timeout;
        loop {
            if self.child.try_wait()?.is_some() {
                return self.child.wait_with_output();
            }

            if Instant::now() >= deadline {
                self.child.kill()?;
                return self.child.wait_with_output();
            }

            thread::sleep(Duration::from_millis(10));
        }
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
        self.entries.insert(handle, SpawnedCommandEntry { plugin_id, state });
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
