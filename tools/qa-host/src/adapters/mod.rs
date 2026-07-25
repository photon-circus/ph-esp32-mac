//! Concrete subprocess and opt-in Windows hardware adapters.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use crate::{
    config::{CommandArgv, LabConfig},
    evidence::{ActionRecord, ToolVersion},
    orchestrator::{ActionRunner, Flasher},
};

#[cfg(all(windows, feature = "windows-hardware"))]
pub mod windows;

/// Runs literal argv commands without a command shell.
#[derive(Default)]
pub struct CommandActionRunner;

impl ActionRunner for CommandActionRunner {
    fn run(
        &mut self,
        action: &str,
        command: &CommandArgv,
        base_dir: &Path,
        timeout: Duration,
    ) -> Result<ActionRecord> {
        let executable = resolve_executable(command, base_dir);
        let started = Instant::now();
        let mut process = Command::new(&executable);
        process.args(command.arguments()).current_dir(base_dir);
        let output = run_with_timeout(&mut process, timeout)
            .with_context(|| format!("run host action `{action}`"))?;
        let success = output.status.success();
        let record = ActionRecord {
            action: action.to_owned(),
            executable: executable
                .file_name()
                .unwrap_or_else(|| executable.as_os_str())
                .to_string_lossy()
                .into_owned(),
            exit_code: output.status.code(),
            duration_ms: started.elapsed().as_millis(),
            success,
            suite: None,
            run_id: None,
            step: None,
            count: None,
        };
        if !success {
            bail!(
                "action `{action}` exited with {}; stdout/stderr: {}",
                output.status,
                output.combined_text()
            );
        }
        Ok(record)
    }
}

/// `espflash` subprocess adapter.
#[derive(Default)]
pub struct EspflashFlasher;

impl Flasher for EspflashFlasher {
    fn flash(&mut self, firmware: &Path, lab: &LabConfig) -> Result<()> {
        let executable = lab.resolve_executable(&lab.tools.espflash);
        let mut command = Command::new(executable);
        command
            .args(lab.tools.espflash.arguments())
            .args(["flash", "--chip", "esp32", "--port"])
            .arg(&lab.serial.port)
            .arg("--baud")
            .arg(lab.serial.baud.to_string())
            .args(["--non-interactive", "--after", "no-reset"])
            .arg(firmware)
            .current_dir(lab.base_dir());
        let output = run_with_timeout(&mut command, Duration::from_millis(lab.timeouts.flash_ms))
            .context("flash QA firmware")?;
        if !output.status.success() {
            bail!(
                "espflash exited with {}; stdout/stderr: {}",
                output.status,
                output.combined_text()
            );
        }
        Ok(())
    }
}

/// Captures the version of the configured `espflash` executable.
#[must_use]
pub fn espflash_version(lab: &LabConfig) -> ToolVersion {
    let executable = lab.resolve_executable(&lab.tools.espflash);
    let mut command = Command::new(executable);
    command
        .args(lab.tools.espflash.arguments())
        .arg("--version")
        .current_dir(lab.base_dir());
    let version = command
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
        .unwrap_or_else(|| "unavailable".to_owned());
    ToolVersion {
        name: "espflash-configured".to_owned(),
        version,
    }
}

fn resolve_executable(command: &CommandArgv, base_dir: &Path) -> PathBuf {
    let path = Path::new(command.executable());
    if path.is_absolute()
        || (!command.executable().contains('/') && !command.executable().contains('\\'))
    {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

struct TimedOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl TimedOutput {
    fn combined_text(&self) -> String {
        let mut bytes = self.stdout.clone();
        bytes.extend_from_slice(&self.stderr);
        String::from_utf8_lossy(&bytes).trim().to_owned()
    }
}

fn run_with_timeout(command: &mut Command, timeout: Duration) -> Result<TimedOutput> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().context("capture child stdout")?;
    let stderr = child.stderr.take().context("capture child stderr")?;
    let stdout_thread = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut stream = stdout;
        stream.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_thread = thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut stream = stderr;
        stream.read_to_end(&mut bytes).map(|_| bytes)
    });

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            bail!("process exceeded {} ms", timeout.as_millis());
        }
        thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout_thread
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader panicked"))??;
    let stderr = stderr_thread
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader panicked"))??;
    Ok(TimedOutput {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_executable;
    use crate::config::CommandArgv;
    use std::path::Path;

    #[test]
    fn bare_executables_remain_path_searches() {
        let command = CommandArgv::new(vec!["relay.exe".to_owned(), "off".to_owned()]).unwrap();
        assert_eq!(
            resolve_executable(&command, Path::new("C:/lab")),
            Path::new("relay.exe")
        );
    }

    #[test]
    fn relative_executable_paths_use_lab_directory() {
        let command = CommandArgv::new(vec!["bin/relay.exe".to_owned(), "off".to_owned()]).unwrap();
        assert_eq!(
            resolve_executable(&command, Path::new("C:/lab")),
            Path::new("C:/lab/bin/relay.exe")
        );
    }
}
