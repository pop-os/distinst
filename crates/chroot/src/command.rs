use std::{
    ffi::OsStr,
    io::{self, BufRead, BufReader, Error, ErrorKind, Write},
    process::{self, Child, ExitStatus, Stdio},
};

/// Convenient wrapper around `process::Command` to make it easier to work with.
pub struct Command<'a> {
    cmd:   process::Command,
    stdin: Option<&'a str>,
}

impl<'a> Command<'a> {
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self {
        Command { cmd: process::Command::new(program), stdin: None }
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Command<'a> {
        self.cmd.arg(arg);
        self
    }

    pub fn args<S: AsRef<OsStr>, I: IntoIterator<Item = S>>(
        &mut self,
        args: I,
    ) -> &mut Command<'a> {
        self.cmd.args(args);
        self
    }

    pub fn env(&mut self, key: &str, value: &str) { self.cmd.env(key, value); }

    pub fn env_clear(&mut self) { self.cmd.env_clear(); }

    pub fn stdin(&mut self, stdio: Stdio) -> &mut Self {
        self.cmd.stdin(stdio);
        self
    }

    pub fn stderr(&mut self, stdio: Stdio) -> &mut Self {
        self.cmd.stderr(stdio);
        self
    }

    pub fn stdout(&mut self, stdio: Stdio) -> &mut Self {
        self.cmd.stdout(stdio);
        self
    }

    /// Run the program, check the status, and get the output of `stdout`
    pub fn stdin_input(mut self, input: &'a str) -> Self {
        self.stdin = Some(input);
        self.stdin(Stdio::piped());
        self
    }

    pub fn stdin_redirect(&mut self, child: &mut Child) -> io::Result<()> {
        match self.stdin {
            Some(input) => child.stdin.as_mut().unwrap().write_all(input.as_bytes()),
            None => Ok(()),
        }
    }

    pub fn run_with_stdout(&mut self) -> io::Result<String> {
        let cmd = format!("{:?}", self.cmd);
        info!("running {}", cmd);

        self.cmd.stdout(Stdio::piped());

        let mut child = self.cmd.spawn().map_err(|why| {
            Error::new(why.kind(), format!("failed to spawn process {}: {}", cmd, why))
        })?;

        self.stdin_redirect(&mut child)?;

        child
            .wait_with_output()
            .map_err(|why| {
                Error::new(why.kind(), format!("failed to get output of {}: {}", cmd, why))
            })
            .and_then(|output| {
                String::from_utf8(output.stdout).map_err(|why| {
                    Error::new(
                        ErrorKind::Other,
                        format!("command output has invalid UTF-8: {}", why),
                    )
                })
            })
    }

    /// Run the program and check the status.
    pub fn run(&mut self) -> io::Result<()> {
        self.run_with_callbacks(|info| info!("{}", info), |error| warn!("{}", error))
    }

    /// Run the program and check the status.
    pub fn run_with_callbacks<I, E>(&mut self, info: I, error: E) -> io::Result<()>
    where
        I: Fn(&str),
        E: Fn(&str),
    {
        let cmd = format!("{:?}", self.cmd);
        info!("running {}", cmd);

        let mut child = self.cmd.spawn().map_err(|why| {
            Error::new(why.kind(), format!("failed to spawn process {}: {}", cmd, why))
        })?;

        self.stdin_redirect(&mut child)?;

        enum Message {
            Stdout(String),
            Stderr(String),
        }

        let (tx, rx) = std::sync::mpsc::channel();

        if let Some(stdout) = child.stdout.take() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let stdout = BufReader::new(stdout);
                for line in stdout.lines().filter_map(Result::ok) {
                    let _res = tx.send(Message::Stdout(line));
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                let stderr = BufReader::new(stderr);
                for line in stderr.lines().filter_map(Result::ok) {
                    let _ = tx.send(Message::Stderr(line));
                }
            });
        }

        for message in rx {
            match message {
                Message::Stdout(line) => info(&line),
                Message::Stderr(line) => error(&line),
            }
        }

        child.wait().and_then(|status| status_as_result(status, &cmd))
    }
}

fn status_as_result(status: ExitStatus, cmd: &str) -> io::Result<()> {
    if status.success() {
        Ok(())
    } else if let Some(127) = status.code() {
        Err(io::Error::new(io::ErrorKind::NotFound, format!("command {} was not found", cmd)))
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("command failed with exit status: {}", status),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_not_found() {
        assert!(Command::new("asdfasdf").run().unwrap_err().kind() == io::ErrorKind::NotFound);
    }

    #[test]
    fn command_with_output() {
        assert_eq!(
            Command::new("echo").arg("Hello, Command!").run_with_stdout().unwrap(),
            "Hello, Command!\n".to_owned()
        );
    }
}
