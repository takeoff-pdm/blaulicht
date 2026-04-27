use std::ffi::OsStr;
use std::io;
use std::process::{Child, Command as StdCommand, Output, Stdio};

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
}
