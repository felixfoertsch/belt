use chrono::Utc;
use colored::Colorize;

use crate::cache::{self, CachedStatus};
use crate::registry::Asteroid;
use crate::ssh;

/// Remote script sent to the asteroid via SSH to collect status data.
/// Mirrors the bash heredoc from the original `asteroids` script.
const REMOTE_STATUS_SCRIPT: &str = r#"
version="$1"
set +e +u
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$HOME/bin:$HOME/.local/bin:$PATH"

if [ "$version" = "8" ]; then
	uberspace web backend list  </dev/null > "$tmpdir/ports" 2>"$tmpdir/e.ports" &
else
	uberspace port list         </dev/null > "$tmpdir/ports" 2>"$tmpdir/e.ports" &
fi
uberspace web domain list  </dev/null > "$tmpdir/web"   2>"$tmpdir/e.web"   &
uberspace mail domain list </dev/null > "$tmpdir/mdom"  2>"$tmpdir/e.mdom"  &
if [ "$version" = "8" ]; then
	uberspace mail address list </dev/null > "$tmpdir/musr"  2>"$tmpdir/e.musr"  &
else
	uberspace mail user list    </dev/null > "$tmpdir/musr"  2>"$tmpdir/e.musr"  &
fi
wait

for _ef in "$tmpdir"/e.*; do
	[ -s "$_ef" ] && cat "$_ef" >&2
done

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

/// Refresh status for a single asteroid via SSH, cache the result, and return it.
pub fn refresh_one(asteroid: &Asteroid) -> Result<CachedStatus, String> {
	eprintln!(
		"{} refreshing status for {} @ {} ...",
		"==>".cyan(),
		asteroid.name,
		asteroid.server
	);

	let raw = ssh::capture(asteroid, REMOTE_STATUS_SCRIPT)?;
	let updated = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();

	let mut ports = Vec::new();
	let mut web = Vec::new();
	let mut mdom = Vec::new();
	let mut musr = Vec::new();

	for line in raw.lines() {
		let line = line.trim_end_matches('\r');
		if let Some(val) = line.strip_prefix("PORTS=") {
			ports = parse_csv(val);
		} else if let Some(val) = line.strip_prefix("WEB=") {
			web = parse_csv(val);
		} else if let Some(val) = line.strip_prefix("MDOM=") {
			mdom = parse_csv(val);
		} else if let Some(val) = line.strip_prefix("MUSR=") {
			musr = parse_csv(val);
		}
	}

	if ports.is_empty() && web.is_empty() && mdom.is_empty() && musr.is_empty() {
		eprintln!(
			"{} no data from {} — check stderr above",
			"[warn]".yellow(),
			asteroid.name
		);
	}

	let status = CachedStatus {
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
pub fn show_all(refresh: bool, asteroids: &[Asteroid]) -> Result<(), String> {
	if refresh {
		for asteroid in asteroids {
			match refresh_one(asteroid) {
				Ok(status) => {
					println!();
					let age = compute_age(&status.updated);
					print_status(&status, &age);
				}
				Err(e) => {
					eprintln!("{} {}: {e}", "[error]".red(), asteroid.name);
				}
			}
		}
		return Ok(());
	}

	let cached = cache::load_all()?;
	if cached.is_empty() {
		eprintln!(
			"{} no cached status — run: {} {}",
			"[warn]".yellow(),
			"uc".blue(),
			"status --refresh".blue()
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
			"{} {stale_count} account(s) have stale cache (>24h) — run: uc status --refresh",
			"[warn]".yellow()
		);
	}

	Ok(())
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
