//! Running external tools. Error messages name the program, never its arguments, so a command
//! line that carries an account name cannot leak into logs through an error chain.

use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// Runs `cmd` with inherited stdio; fails unless it exits with status 0.
pub(crate) fn run(cmd: &mut Command) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let status = cmd.status().with_context(|| format!("cannot run `{program}`"))?;
    if !status.success() {
        bail!("`{program}` failed ({status})");
    }
    Ok(())
}

/// Runs `cmd`, returning its stdout. On failure the error carries the tool's stderr and stdout.
pub(crate) fn output(cmd: &mut Command) -> Result<String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("cannot run `{program}`"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!(
            "`{program}` failed ({}):\n{}{}",
            out.status,
            stderr.trim_end(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!("\n{}", stdout.trim_end())
            },
        );
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_reports_failures_by_program_name() {
        run(&mut Command::new("true")).expect("true succeeds");
        let err = run(&mut Command::new("false")).expect_err("false fails");
        assert!(err.to_string().starts_with("`false` failed"), "{err}");
        let err = run(&mut Command::new("definitely-not-a-program-xtask")).expect_err("missing");
        assert!(err.to_string().contains("cannot run"), "{err}");
    }

    #[test]
    fn output_returns_stdout_and_error_text_on_failure() {
        let ok = output(Command::new("echo").arg("hi")).expect("echo");
        assert_eq!(ok, "hi\n");
        let err = output(Command::new("sh").args(["-c", "echo oops >&2; exit 3"])).expect_err("exit 3");
        let text = err.to_string();
        assert!(text.contains("oops") && text.contains("`sh` failed"), "{text}");
        // the arguments (here the script) are not part of the message
        assert!(!text.contains("exit 3"), "{text}");
    }
}
