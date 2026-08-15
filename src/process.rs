use std::ffi::OsStr;
use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1_500);

pub(crate) fn command_output<I, S>(argv: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    command_output_with_timeout(argv, DEFAULT_TIMEOUT)
}

pub(crate) fn command_output_with_timeout<I, S>(argv: I, timeout: Duration) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut parts = argv.into_iter();
    let command = parts.next()?;
    let mut child = Command::new(command)
        .args(parts)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Polling while the child fills the stdout pipe would deadlock.
    let mut stdout = child.stdout.take()?;
    let reader = thread::spawn(move || {
        let mut buffer = String::new();
        stdout.read_to_string(&mut buffer).ok().map(|_| buffer)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let output = reader.join().ok().flatten()?;
    if !status?.success() {
        return None;
    }
    Some(output.trim().to_owned())
}

pub(crate) fn run_command(argv: &[&str], timeout: Duration) -> io::Result<i32> {
    if argv.is_empty() {
        return Ok(1);
    }
    let mut child = Command::new(argv[0])
        .args(&argv[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.code().unwrap_or(1));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(124);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

pub(crate) fn run_command_owned(argv: &[String], timeout: Duration) -> io::Result<i32> {
    let refs = argv.iter().map(String::as_str).collect::<Vec<_>>();
    run_command(&refs, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_output_for_a_fast_command() {
        assert_eq!(command_output(["echo", "hello"]).as_deref(), Some("hello"));
    }

    #[test]
    fn gives_up_on_a_command_that_outlives_the_deadline() {
        let start = Instant::now();
        assert!(command_output_with_timeout(["sleep", "5"], Duration::from_millis(200)).is_none());
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
