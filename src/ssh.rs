use std::process::Command;

use crate::registry::Asteroid;

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteOutput {
	pub stdout: String,
	pub stderr: String,
	pub exit_code: i32,
}

/// Run remote `uberspace` with inherited stdio. SSH config supplies aliases,
/// identity files, and host-key policy. A TTY is requested only from a terminal.
pub fn passthrough(asteroid: &Asteroid, args: &[String]) -> Result<i32, String> {
	use std::io::IsTerminal;

	let mut cmd = Command::new("ssh");
	cmd.args(base_args());
	if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
		cmd.arg("-t");
	}
	cmd.arg(host(asteroid)).arg("uberspace").args(args);

	let status = cmd
		.status()
		.map_err(|e| format!("failed to execute ssh: {e}"))?;

	Ok(status.code().unwrap_or(1))
}

/// Send a Bash script through stdin so remote login-shell quoting cannot alter it.
pub fn capture(asteroid: &Asteroid, remote_script: &str, args: &[String]) -> Result<RemoteOutput, String> {
	let mut cmd = Command::new("ssh");
	cmd.args(base_args())
		.arg(host(asteroid))
		.arg("bash")
		.arg("-s")
		.arg("--")
		.args(args)
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped());

	let output = cmd
		.spawn()
		.and_then(|mut child| {
			use std::io::Write;
			if let Some(ref mut stdin) = child.stdin {
				stdin.write_all(remote_script.as_bytes())?;
			}
			child.wait_with_output()
		})
		.map_err(|e| format!("ssh to {} failed: {e}", asteroid.name))?;

	Ok(RemoteOutput {
		stdout: String::from_utf8(output.stdout)
			.map_err(|e| format!("invalid utf-8 from ssh stdout: {e}"))?,
		stderr: String::from_utf8(output.stderr)
			.map_err(|e| format!("invalid utf-8 from ssh stderr: {e}"))?,
		exit_code: output.status.code().unwrap_or(1),
	})
}

fn base_args() -> [&'static str; 10] {
	[
		"-o",
		"BatchMode=yes",
		"-o",
		"ConnectTimeout=10",
		"-o",
		"ServerAliveInterval=15",
		"-o",
		"ServerAliveCountMax=2",
		"-o",
		"StrictHostKeyChecking=yes",
	]
}

fn host(asteroid: &Asteroid) -> String {
	format!("{}@{}", asteroid.name, asteroid.server)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn automation_ssh_options_are_bounded_and_strict() {
		let args = base_args();
		assert!(args.contains(&"BatchMode=yes"));
		assert!(args.contains(&"ConnectTimeout=10"));
		assert!(args.contains(&"ServerAliveCountMax=2"));
		assert!(args.contains(&"StrictHostKeyChecking=yes"));
	}

	#[test]
	fn host_includes_remote_account() {
		let asteroid = Asteroid {
			name: "danger".into(),
			server: "cetus.uberspace.de".into(),
			version: 7,
		};
		assert_eq!(host(&asteroid), "danger@cetus.uberspace.de");
	}
}
