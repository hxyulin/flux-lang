//! Bounded subprocesses with file-backed output and persistent command diagnostics.
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        // Reap even when a filesystem or polling error unwinds the harness.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub struct Run {
    pub output: Output,
    pub timed_out: bool,
}
impl Run {
    /// Ordinary fixtures fail immediately; audit parents instead inspect
    /// `output` after their source-integrity guard has run.
    pub fn completed(self) -> Output {
        assert!(
            !self.timed_out,
            "command timed out; see its .command and .status artifacts"
        );
        self.output
    }
}

pub fn run(command: &mut Command, directory: &Path, label: &str, limit: Duration) -> Run {
    let stdout = directory.join(format!("{label}.stdout"));
    let stderr = directory.join(format!("{label}.stderr"));
    std::fs::write(
        directory.join(format!("{label}.command")),
        format!("{command:?}\nlimit_seconds={}\n", limit.as_secs_f64()),
    )
    .expect("write command artifact");
    let mut child = Running(
        command
            .stdout(Stdio::from(
                std::fs::File::create(&stdout).expect("create stdout artifact"),
            ))
            .stderr(Stdio::from(
                std::fs::File::create(&stderr).expect("create stderr artifact"),
            ))
            .spawn()
            .unwrap_or_else(|error| panic!("cannot spawn {command:?}: {error}")),
    );
    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("poll child") {
            break status;
        }
        if start.elapsed() > limit {
            timed_out = true;
            child.0.kill().expect("kill timed-out child");
            break child.0.wait().expect("reap timed-out child");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    std::fs::write(
        directory.join(format!("{label}.status")),
        format!(
            "status={status}\ntimeout={timed_out}\nelapsed={:?}\n",
            start.elapsed()
        ),
    )
    .expect("write status artifact");
    Run {
        timed_out,
        output: Output {
            status,
            stdout: std::fs::read(stdout).expect("read stdout artifact"),
            stderr: std::fs::read(stderr).expect("read stderr artifact"),
        },
    }
}
