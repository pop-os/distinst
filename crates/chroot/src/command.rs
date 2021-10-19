use libc;
use std::{
    ffi::OsStr,
    fs::File,
    io::{self, BufRead, BufReader, Error, ErrorKind, Write},
    os::unix::io::{FromRawFd, IntoRawFd},
    process::{self, Child, ExitStatus, Stdio},
    thread,
    time::Duration,
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

        let mut stdout_buffer = String::new();
        let mut stdout = child.stdout.take().map(non_blocking).map(BufReader::new);

        let mut stderr_buffer = String::new();
        let mut stderr = child.stderr.take().map(non_blocking).map(BufReader::new);

        loop {
            thread::sleep(Duration::from_millis(16));
            match child.try_wait()? {
                Some(status) => return status_as_result(status, &cmd),
                None => {
                    if let Some(ref mut stdout) = stdout {
                        non_blocking_line_reading(stdout, &mut stdout_buffer, &info)?;
                    }

                    if let Some(ref mut stderr) = stderr {
                        non_blocking_line_reading(stderr, &mut stderr_buffer, &error)?;
                    }
                }
            }
        }
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

fn non_blocking<F: IntoRawFd>(fd: F) -> File {
    let fd = fd.into_raw_fd();
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        File::from_raw_fd(fd)
    }
}

fn non_blocking_line_reading<B: BufRead, F: Fn(&str)>(
    reader: &mut B,
    buffer: &mut String,
    callback: F,
) -> io::Result<()> {
    loop {
        match reader.read_line(buffer) {
            Ok(0) => break,
            Ok(read) => {
                if buffer.is_char_boundary(read) {
                    callback(&buffer[..read - 1]);
                    buffer.clear();
                }
            }
            Err(ref why) if why.kind() == io::ErrorKind::WouldBlock => break,
            Err(why) => {
                buffer.clear();
                return Err(why);
            }
        }
    }

    Ok(())
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
