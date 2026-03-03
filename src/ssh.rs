use std::process::Command;

use crate::registry::Asteroid;

/// Run `ssh -t <name>@<server> uberspace <args...>` and inherit stdio.
/// Returns the exit code from ssh.
pub fn passthrough(asteroid: &Asteroid, args: &[String]) -> Result<i32, String> {
	let host = format!("{}@{}", asteroid.name, asteroid.server);

	let mut cmd = Command::new("ssh");
	cmd.arg("-t").arg("-o").arg("BatchMode=no").arg(&host);
	cmd.arg("uberspace");
	cmd.args(args);

	let status = cmd
		.status()
		.map_err(|e| format!("failed to execute ssh: {e}"))?;

	Ok(status.code().unwrap_or(1))
}

/// Run a command on the remote via SSH and capture stdout.
/// Used for status collection where we need to parse output.
pub fn capture(asteroid: &Asteroid, remote_script: &str) -> Result<String, String> {
	let host = format!("{}@{}", asteroid.name, asteroid.server);

	let output = Command::new("ssh")
		.arg("-o")
		.arg("BatchMode=no")
		.arg(&host)
		.arg("bash")
		.arg("-s")
		.arg(asteroid.version.to_string())
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::inherit())
		.spawn()
		.and_then(|mut child| {
			use std::io::Write;
			if let Some(ref mut stdin) = child.stdin {
				stdin.write_all(remote_script.as_bytes())?;
			}
			child.wait_with_output()
		})
		.map_err(|e| format!("ssh to {} failed: {e}", asteroid.name))?;

	if !output.status.success() {
		return Err(format!(
			"ssh to {}@{} failed with exit code {}",
			asteroid.name,
			asteroid.server,
			output.status.code().unwrap_or(1)
		));
	}

	String::from_utf8(output.stdout)
		.map_err(|e| format!("invalid utf-8 from ssh output: {e}"))
}
