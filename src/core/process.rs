use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};

use std::sync::Arc;

use crate::core::app_log::AppLog;
use crate::core::cancel::CancelToken;
use crate::core::error::AppError;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub type LogSink = Arc<dyn Fn(String) + Send + Sync>;

pub struct CommandOutput {
    pub success: bool,
    pub text: String,
}

pub fn capture(program: &str, arguments: &[String], timeout: CancelToken) -> CommandOutput {
    let Ok(mut child) = spawn(program, arguments, false) else {
        return CommandOutput {
            success: false,
            text: String::new(),
        };
    };
    let stdout = child.stdout.take().map(collect);
    let stderr = child.stderr.take().map(collect);
    let completed = wait_for(&mut child, &timeout);
    if !completed {
        return CommandOutput {
            success: false,
            text: String::new(),
        };
    }
    let status = child.wait().ok();
    let mut text = stdout
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    text.push_str(
        &stderr
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default(),
    );
    CommandOutput {
        success: status.map(|value| value.success()).unwrap_or(false),
        text,
    }
}

pub fn run_logged(
    program: &str,
    arguments: &[String],
    timeout: CancelToken,
    log: &LogSink,
    failure: &str,
) -> Result<(), AppError> {
    let mut child = spawn(program, arguments, false).map_err(|error| {
        AppError::python(format!("{failure} {}", AppLog::redact(&error.to_string())))
    })?;
    let stdout = child.stdout.take().map(|value| drain(value, log));
    let stderr = child.stderr.take().map(|value| drain(value, log));
    let completed = wait_for(&mut child, &timeout);
    if let Some(handle) = stdout {
        let _ = handle.join();
    }
    if let Some(handle) = stderr {
        let _ = handle.join();
    }
    if !completed {
        return Err(AppError::python("Python command timed out."));
    }
    let status = child
        .wait()
        .map_err(|error| AppError::python(AppLog::redact(&error.to_string())))?;
    if !status.success() {
        return Err(AppError::python(format!(
            "Python command failed with code {}.",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

pub struct TranscriptionProcess {
    child: Child,
    stdin: Option<std::process::ChildStdin>,
    stdout: Option<BufReader<std::process::ChildStdout>>,
}

impl TranscriptionProcess {
    pub fn spawn_transcription(program: &str, runner: &str) -> Result<Self, AppError> {
        let mut child = Command::new(program)
            .arg(runner)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONUTF8", "1")
            .env("PYTHONUNBUFFERED", "1")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| {
                AppError::python(format!(
                    "Unable to start the transcription runner. {}",
                    AppLog::redact(&error.to_string())
                ))
            })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().map(BufReader::new);
        if let Some(reader) = child.stderr.take().map(BufReader::new) {
            std::thread::spawn(move || {
                for line in reader.lines().map_while(Result::ok) {
                    if !line.is_empty() {
                        AppLog::info(&AppLog::redact(&line));
                    }
                }
            });
        }
        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }

    pub fn write_payload(&mut self, payload: &str) -> Result<(), AppError> {
        let Some(handle) = self.stdin.as_mut() else {
            return Err(AppError::python("The runner input stream is unavailable."));
        };
        handle
            .write_all(payload.as_bytes())
            .and_then(|_| handle.flush())
            .map_err(|error| AppError::python(AppLog::redact(&error.to_string())))?;
        self.stdin = None;
        Ok(())
    }

    pub fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        let handle = self.stdout.as_mut()?;
        match handle.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(line.trim_end().to_string()),
            Err(_) => None,
        }
    }

    pub fn wait(&mut self) -> Option<i32> {
        self.child.wait().ok().and_then(|status| status.code())
    }

    pub fn terminate(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn(program: &str, arguments: &[String], with_stdin: bool) -> std::io::Result<Child> {
    Command::new(program)
        .args(arguments)
        .stdin(match with_stdin {
            true => Stdio::piped(),
            false => Stdio::null(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("PYTHONUTF8", "1")
        .env("PYTHONUNBUFFERED", "1")
        .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
        .env("PIP_NO_INPUT", "1")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
}

fn wait_for(child: &mut Child, timeout: &CancelToken) -> bool {
    loop {
        if timeout.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(_) => return false,
        }
    }
}

fn collect(mut reader: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = reader.read_to_string(&mut buffer);
        buffer
    })
}

fn drain(reader: impl Read + Send + 'static, log: &LogSink) -> std::thread::JoinHandle<()> {
    let reader = BufReader::new(reader);
    let sink = Arc::clone(log);
    std::thread::spawn(move || {
        for line in reader.lines().map_while(Result::ok) {
            if !line.is_empty() {
                sink(AppLog::redact(&line));
            }
        }
    })
}
