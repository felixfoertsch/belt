use chrono::Utc;
use colored::Colorize;

use crate::cache::{self, CachedStatus};
use crate::registry::Asteroid;
use crate::ssh;

/// Remote script sent to the asteroid via SSH to collect status data.
/// Mirrors the bash heredoc from the original `asteroids` script.
const REMOTE_STATUS_SCRIPT: &str = r#"
version="$1"
set -eu
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$HOME/bin:$HOME/.local/bin:$PATH"

run() {
	timeout 20 uberspace "$@" </dev/null
}

if [ "$version" = "8" ]; then
	run web backend list > "$tmpdir/ports"
else
	run port list > "$tmpdir/ports"
fi
run web domain list > "$tmpdir/web"
run mail domain list > "$tmpdir/mdom"
if [ "$version" = "8" ]; then
	run mail address list > "$tmpdir/musr"
else
	run mail user list > "$tmpdir/musr"
fi

parse_list() {
	local f="$1"
	[ -s "$f" ] || return 0
	if grep -q $'\xe2\x94' "$f" 2>/dev/null; then
		grep -v $'\xe2\x94' "$f" \
			| awk 'NR==1{next} /^[[:space:]]*$/{next} {gsub(/^[[:space:]]+|[[:space:]]+$/,""); print}'
	else
		grep -v '^[[:space:]]*$' "$f"
	fi
}

ports=$(parse_list "$tmpdir/ports" | tr '\n' ',' | sed 's/,$//')
web=$(parse_list   "$tmpdir/web"   | tr '\n' ',' | sed 's/,$//')
mdom=$(parse_list  "$tmpdir/mdom"  | tr '\n' ',' | sed 's/,$//')
musr=$(parse_list  "$tmpdir/musr"  | tr '\n' ',' | sed 's/,$//')

printf 'PORTS=%s\n'   "$ports"
printf 'WEB=%s\n'     "$web"
printf 'MDOM=%s\n'    "$mdom"
printf 'MUSR=%s\n'    "$musr"
"#;

const DETECT_VERSION_SCRIPT: &str = r#"
set -eu
. /etc/os-release
case "${ID:-}:${VERSION_ID:-}" in
	centos:7|centos:7.*) printf '7\n' ;;
	arch:*) printf '8\n' ;;
	*) printf 'unsupported operating system: ID=%s VERSION_ID=%s\n' "${ID:-}" "${VERSION_ID:-}" >&2; exit 64 ;;
esac
"#;

fn parse_detected_version(output: &ssh::RemoteOutput) -> Result<u8, String> {
	if output.exit_code != 0 {
		let detail = output.stderr.trim();
		return Err(if detail.is_empty() {
			format!("remote generation detection failed with exit code {}", output.exit_code)
		} else {
			format!("remote generation detection failed: {detail}")
		});
	}
	match output.stdout.trim() {
		"7" => Ok(7),
		"8" => Ok(8),
		other => Err(format!("invalid generation response: {other:?}")),
	}
}

pub fn detect_version(asteroid: &Asteroid) -> Result<u8, String> {
	let output = ssh::capture(asteroid, DETECT_VERSION_SCRIPT, &[])?;
	parse_detected_version(&output)
}

fn remote_failure(asteroid: &Asteroid, output: &ssh::RemoteOutput) -> String {
	let detail = output.stderr.trim();
	if detail.is_empty() {
		format!(
			"remote command on {}@{} failed with exit code {}",
			asteroid.name, asteroid.server, output.exit_code
		)
	} else {
		format!(
			"remote command on {}@{} failed with exit code {}: {detail}",
			asteroid.name, asteroid.server, output.exit_code
		)
	}
}

/// Refresh status for a single asteroid via SSH, cache the result, and return it.
pub fn refresh_one(asteroid: &Asteroid) -> Result<CachedStatus, String> {
	eprintln!(
		"{} refreshing status for {} @ {} ...",
		"==>".cyan(),
		asteroid.name,
		asteroid.server
	);

	let detected_version = detect_version(asteroid)?;
	if detected_version != asteroid.version {
		return Err(format!(
			"registered as U{}, but /etc/os-release reports U{}; update registry before retrying",
			asteroid.version, detected_version
		));
	}
	let output = ssh::capture(
		asteroid,
		REMOTE_STATUS_SCRIPT,
		&[detected_version.to_string()],
	)?;
	if output.exit_code != 0 {
		return Err(remote_failure(asteroid, &output));
	}
	if !output.stderr.trim().is_empty() {
		eprintln!("{}", output.stderr.trim_end());
	}
	let raw = output.stdout;
	let updated = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

	let (ports, web, mdom, musr) = parse_status_output(&raw)?;

	let status = CachedStatus {
		schema_version: 1,
		updated,
		name: asteroid.name.clone(),
		server: asteroid.server.clone(),
		version: asteroid.version,
		ports,
		web_domains: web,
		mail_domains: mdom,
		mail_users: musr,
	};

	cache::save(&status)?;
	Ok(status)
}

/// Display a single status block with colored formatting.
pub fn print_status(status: &CachedStatus, age_display: &str) {
	let ver = format!("u{}", status.version);
	let header_right = format!("{}  {}", status.server, ver);

	// header line
	println!(
		"  {}  {}{}",
		status.name.bold(),
		header_right.dimmed(),
		format_age_suffix(age_display),
	);

	let (ports_label, users_label) = if status.version == 8 {
		("backends", "addrs")
	} else {
		("ports", "users")
	};

	print_field(ports_label, &status.ports);
	print_field("web", &status.web_domains);
	print_field("mail", &status.mail_domains);
	print_field(users_label, &status.mail_users);
}

/// Show aggregate status from cache for all asteroids.
pub fn show_all(refresh: bool, json: bool, asteroids: &[Asteroid]) -> Result<(), String> {
	if refresh {
		let mut refreshed = Vec::new();
		for asteroid in asteroids {
			match refresh_one(asteroid) {
				Ok(status) => {
					if json {
						refreshed.push(status);
					} else {
						println!();
						let age = compute_age(&status.updated);
						print_status(&status, &age);
					}
				}
				Err(e) => {
					eprintln!("{} {}: {e}", "[error]".red(), asteroid.name);
				}
			}
		}
		if json {
			println!(
				"{}",
				serde_json::to_string_pretty(&refreshed)
					.map_err(|e| format!("failed to serialize status: {e}"))?
			);
		}
		return Ok(());
	}

	let mut cached = Vec::new();
	for asteroid in asteroids {
		match cache::load(&asteroid.name)? {
			Some(status)
				if status.server == asteroid.server && status.version == asteroid.version =>
			{
				cached.push(status)
			}
			Some(_) => eprintln!(
				"{} {} cache metadata does not match registry",
				"[warn]".yellow(), asteroid.name
			),
			None => eprintln!(
				"{} {} has no cached status — run: belt status --refresh",
				"[warn]".yellow(), asteroid.name
			),
		}
	}
	if cached.is_empty() {
		return Ok(());
	}
	if json {
		println!(
			"{}",
			serde_json::to_string_pretty(&cached)
				.map_err(|e| format!("failed to serialize status: {e}"))?
		);
		return Ok(());
	}

	let mut stale_count = 0;
	let mut first = true;

	for status in &cached {
		if !first {
			println!();
		}
		first = false;

		let age = compute_age(&status.updated);
		if is_stale(&status.updated) {
			stale_count += 1;
		}
		print_status(status, &age);
	}

	if stale_count > 0 {
		println!();
		eprintln!(
			"{} {stale_count} account(s) have stale cache (>24h) — run: belt status --refresh",
			"[warn]".yellow()
		);
	}

	Ok(())
}

fn parse_status_output(raw: &str) -> Result<(Vec<String>, Vec<String>, Vec<String>, Vec<String>), String> {
	let mut fields: [Option<Vec<String>>; 4] = [None, None, None, None];
	for line in raw.lines() {
		let line = line.trim_end_matches('\r');
		let (index, value) = if let Some(value) = line.strip_prefix("PORTS=") {
			(0, value)
		} else if let Some(value) = line.strip_prefix("WEB=") {
			(1, value)
		} else if let Some(value) = line.strip_prefix("MDOM=") {
			(2, value)
		} else if let Some(value) = line.strip_prefix("MUSR=") {
			(3, value)
		} else {
			continue;
		};
		if fields[index].is_some() {
			return Err(format!("duplicate status field: {line}"));
		}
		fields[index] = Some(parse_csv(value));
	}
	let [Some(ports), Some(web), Some(mdom), Some(musr)] = fields else {
		return Err("incomplete remote status response".into());
	};
	Ok((ports, web, mdom, musr))
}

fn print_field(label: &str, values: &[String]) {
	if values.is_empty() {
		println!("  {}  (none)", format!("{label:<9}").dimmed());
	} else if values.len() == 1 {
		println!("  {}  {}", format!("{label:<9}").dimmed(), values[0]);
	} else {
		println!(
			"  {}  \u{251c}\u{2500}\u{2500} {}",
			format!("{label:<9}").dimmed(),
			values[0]
		);
		for v in &values[1..values.len() - 1] {
			println!("  {}  \u{2502}   {v}", " ".repeat(9).dimmed());
		}
		println!(
			"  {}  \u{2514}\u{2500}\u{2500} {}",
			" ".repeat(9).dimmed(),
			values[values.len() - 1]
		);
	}
}

fn parse_csv(s: &str) -> Vec<String> {
	if s.is_empty() {
		return Vec::new();
	}
	s.split(',')
		.map(|s| s.trim().to_string())
		.filter(|s| !s.is_empty())
		.collect()
}

fn compute_age(updated: &str) -> String {
	let Ok(ts) = chrono::NaiveDateTime::parse_from_str(updated, "%Y-%m-%dT%H:%M:%S") else {
		return updated.to_string();
	};
	let now = Utc::now().naive_utc();
	let age = now.signed_duration_since(ts);
	let secs = age.num_seconds();

	if secs < 0 {
		return "just now".into();
	} else if secs < 60 {
		format!("{secs}s ago")
	} else if secs < 3600 {
		format!("{}m ago", secs / 60)
	} else if secs < 86400 {
		format!("{}h ago", secs / 3600)
	} else {
		format!("{}d ago", secs / 86400)
	}
}

fn is_stale(updated: &str) -> bool {
	let Ok(ts) = chrono::NaiveDateTime::parse_from_str(updated, "%Y-%m-%dT%H:%M:%S") else {
		return true;
	};
	let now = Utc::now().naive_utc();
	let age = now.signed_duration_since(ts);
	age.num_seconds() >= 86400
}

fn format_age_suffix(age: &str) -> String {
	if age.ends_with("d ago") {
		format!("  {} {}", age.dimmed(), "!".yellow())
	} else {
		format!("  {}", age.dimmed())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn detects_u7() {
		let output = ssh::RemoteOutput {
			stdout: "7\n".into(),
			stderr: String::new(),
			exit_code: 0,
		};
		assert_eq!(parse_detected_version(&output).unwrap(), 7);
	}

	#[test]
	fn detects_u8() {
		let output = ssh::RemoteOutput {
			stdout: "8\n".into(),
			stderr: String::new(),
			exit_code: 0,
		};
		assert_eq!(parse_detected_version(&output).unwrap(), 8);
	}

	#[test]
	fn rejects_unknown_remote_system() {
		let output = ssh::RemoteOutput {
			stdout: String::new(),
			stderr: "unsupported operating system: ID=debian VERSION_ID=13\n".into(),
			exit_code: 64,
		};
		assert_eq!(
			parse_detected_version(&output).unwrap_err(),
			"remote generation detection failed: unsupported operating system: ID=debian VERSION_ID=13"
		);
	}

	#[test]
	fn rejects_malformed_generation_response() {
		let output = ssh::RemoteOutput {
			stdout: "9\n".into(),
			stderr: String::new(),
			exit_code: 0,
		};
		assert_eq!(
			parse_detected_version(&output).unwrap_err(),
			"invalid generation response: \"9\""
		);
	}

	#[test]
	fn parses_complete_status_output() {
		let parsed = parse_status_output("PORTS=40000\r\nWEB=a.de,b.de\nMDOM=\nMUSR=user\n").unwrap();
		assert_eq!(parsed.0, vec!["40000"]);
		assert_eq!(parsed.1, vec!["a.de", "b.de"]);
		assert!(parsed.2.is_empty());
		assert_eq!(parsed.3, vec!["user"]);
	}

	#[test]
	fn rejects_incomplete_status_output() {
		assert_eq!(
			parse_status_output("PORTS=\nWEB=\n").unwrap_err(),
			"incomplete remote status response"
		);
	}

	#[test]
	fn parse_csv_basic() {
		let result = parse_csv("a.de,b.de,c.de");
		assert_eq!(result, vec!["a.de", "b.de", "c.de"]);
	}

	#[test]
	fn parse_csv_empty() {
		assert!(parse_csv("").is_empty());
	}

	#[test]
	fn age_format() {
		let now = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
		let age = compute_age(&now);
		assert!(age.contains("s ago") || age == "just now");
	}

	#[test]
	fn stale_detection() {
		assert!(is_stale("2020-01-01T00:00:00"));
		let now = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
		assert!(!is_stale(&now));
	}
}
