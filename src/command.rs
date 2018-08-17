use std::process::{self, Stdio};
use std::io::{self, BufRead, BufReader, Error, ErrorKind};
use std::ffi::OsStr;
use std::thread;

pub struct Command(process::Command);

impl Command {
    pub fn new<S: AsRef<OsStr>>(program: S) -> Command {
        Command(process::Command::new(program))
    }

    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Command {
        self.0.arg(arg);
        self
    }

    pub fn args<S: AsRef<OsStr>, I: IntoIterator<Item = S>>(&mut self, args: I) -> &mut Command {
        self.0.args(args);
        self
    }

    pub fn env(&mut self, key: &str, value: &str) {
        self.0.env(key, value);
    }

    pub fn env_clear(&mut self) {
        self.0.env_clear();
    }

    pub fn stdin(&mut self, stdio: Stdio) { self.0.stdin(stdio); }
    pub fn stderr(&mut self, stdio: Stdio) { self.0.stderr(stdio); }
    pub fn stdout(&mut self, stdio: Stdio) { self.0.stdout(stdio); }

    pub fn run_with_stdout(&mut self) -> io::Result<String> {
        info!("running {:?}", self.0);

        let mut child = self.0.spawn().map_err(|why| Error::new(
            ErrorKind::Other,
            format!("chroot command failed to spawn: {}", why)
        ))?;

        child.wait_with_output()
            .and_then(|output| {
                String::from_utf8(output.stdout)
                    .map_err(|why| Error::new(
                        ErrorKind::Other,
                        format!("command output has invalid UTF-8: {}", why)
                    ))
            })
    }

    pub fn run(&mut self) -> io::Result<()> {
        info!("running {:?}", self.0);

        let mut child = self.0.spawn().map_err(|why| Error::new(
            ErrorKind::Other,
            format!("chroot command failed to spawn: {}", why)
        ))?;

        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        thread::spawn(move || {
            let buffer = &mut String::with_capacity(8 * 1024);
            loop {
                buffer.clear();
                match stdout.read_line(buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        info!("{}", buffer.trim_right());
                    }
                }
            }
        });

        let mut stderr = BufReader::new(child.stderr.take().unwrap());
        thread::spawn(move || {
            let buffer = &mut String::with_capacity(8 * 1024);
            loop {
                buffer.clear();
                match stderr.read_line(buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        warn!("{}", buffer.trim_right());
                    }
                }
            }
        });

        let status = child.wait().map_err(|why| Error::new(
            ErrorKind::Other,
            format!("waiting on chroot child process failed: {}", why)
        ))?;

        if status.success() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("command failed with exit status: {}", status)
            ))
        }
    }
}
