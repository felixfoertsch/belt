use std::fs;
use std::process;

use clap::{Parser, Subcommand};
use colored::Colorize;

use crate::cache;
use crate::dashboard;
use crate::registry::{self, Asteroid, Registry};
use crate::ssh;
use crate::status;
use crate::translate;

#[derive(Parser)]
#[command(name = "belt", about = "uberspace account manager")]
struct Cli {
	#[command(subcommand)]
	command: Option<Commands>,

	/// Asteroid name (for passthrough commands)
	#[arg(num_args = 1..)]
	args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
	/// Register an asteroid
	Add {
		/// Account name
		name: String,
		/// Server hostname (e.g. cetus.uberspace.de)
		server: String,
		/// Uberspace version (7 or 8)
		version: u8,
	},

	/// List asteroids from Uberspace dashboard
	List,

	/// Manage Uberspace dashboard account session
	Account {
		#[command(subcommand)]
		command: AccountCommands,
	},

	/// Manage local SSH registry
	Registry {
		#[command(subcommand)]
		command: RegistryCommands,
	},

	/// Deregister an asteroid
	Remove {
		/// Account name
		name: String,
	},

	/// Show aggregate status from cache (or refresh all via SSH)
	Status {
		/// Refresh all accounts via SSH before showing
		#[arg(long)]
		refresh: bool,
		/// Emit machine-readable JSON
		#[arg(long)]
		json: bool,
	},

	/// Import asteroids from the legacy asteroids.list file
	Import {
		/// Path to the legacy asteroids.list file
		path: String,
	},
}

#[derive(Subcommand)]
enum AccountCommands {
	/// Log in to Uberspace dashboard
	Login {
		/// Mail address or username
		#[arg(long)]
		login: String,
		/// Read password from standard input instead of prompting
		#[arg(long)]
		password_stdin: bool,
	},
	/// Log out and remove local dashboard session
	Logout,
}

#[derive(Subcommand)]
enum RegistryCommands {
	/// List local SSH registry
	List,
}

pub fn run() {
	// clap's derive approach doesn't easily support `belt <name> <args...>` alongside
	// named subcommands. We handle this by trying clap first, and falling back to
	// manual parsing for asteroid passthrough.
	let result = match Cli::try_parse() {
		Ok(cli) => dispatch(cli),
		Err(e) => {
			// If clap fails because args don't match a subcommand, try passthrough
			let args: Vec<String> = std::env::args().skip(1).collect();
			if args.is_empty() {
				e.exit();
			}
			// Check if first arg is a registered asteroid
			let reg_path = match registry::registry_path() {
				Ok(p) => p,
				Err(_) => e.exit(),
			};
			let reg = match Registry::load(&reg_path) {
				Ok(r) => r,
				Err(_) => e.exit(),
			};
			if reg.lookup(&args[0]).is_some() {
				handle_passthrough(&args[0], &args[1..])
			} else {
				e.exit()
			}
		}
	};

	if let Err(msg) = result {
		eprintln!("{} {msg}", "[error]".red());
		process::exit(1);
	}
}

fn dispatch(cli: Cli) -> Result<(), String> {
	match cli.command {
		Some(Commands::Add {
			name,
			server,
			version,
		}) => cmd_add(&name, &server, version),
		Some(Commands::List) => cmd_dashboard_list(),
		Some(Commands::Account { command }) => match command {
			AccountCommands::Login { login, password_stdin } => cmd_account_login(&login, password_stdin),
			AccountCommands::Logout => cmd_account_logout(),
		},
		Some(Commands::Registry { command: RegistryCommands::List }) => cmd_registry_list(),
		Some(Commands::Remove { name }) => cmd_remove(&name),
		Some(Commands::Status { refresh, json }) => cmd_status(refresh, json),
		Some(Commands::Import { path }) => cmd_import(&path),
		None => {
			// Fall through to passthrough if args present
			if cli.args.is_empty() {
				Cli::parse_from(["belt", "--help"]);
				Ok(())
			} else {
				handle_passthrough(&cli.args[0], &cli.args[1..])
			}
		}
	}
}

fn cmd_add(name: &str, server: &str, version: u8) -> Result<(), String> {
	registry::validate_asteroid(name, server, version)?;
	let path = registry::registry_path()?;
	let mut reg = Registry::load(&path)?;
	reg.add(Asteroid {
		name: name.to_string(),
		server: server.to_string(),
		version,
	});
	reg.save(&path)?;
	let ver_info = format!("u{version}");
	eprintln!(
		"{} registered: {name} @ {server} ({ver_info})",
		"[ok]".green()
	);
	Ok(())
}

fn cmd_account_login(login: &str, password_stdin: bool) -> Result<(), String> {
	dashboard::login(login, password_stdin)?;
	eprintln!("{} logged in", "[ok]".green());
	Ok(())
}

fn cmd_account_logout() -> Result<(), String> {
	dashboard::logout()?;
	eprintln!("{} logged out", "[ok]".green());
	Ok(())
}

fn cmd_dashboard_list() -> Result<(), String> {
	let asteroids = dashboard::list()?;
	println!("  {}", format!("{:<12}  {:<18}  {:<10}  {:<10}  {:>10}  {:>10}", "NAME", "HOST", "CREATED", "STORAGE", "BALANCE", "PRICE").bold());
	println!("{}", "\u{2500}".repeat(82).dimmed());
	for asteroid in asteroids {
		println!("  {:<12}  {:<18}  {:<10}  {:<10}  {:>10}  {:>10}", asteroid.name, asteroid.hostname, asteroid.created, asteroid.storage, asteroid.balance, asteroid.price);
	}
	Ok(())
}

fn cmd_registry_list() -> Result<(), String> {
	let path = registry::registry_path()?;
	let reg = Registry::load(&path)?;
	if reg.asteroid.is_empty() {
		eprintln!(
			"{} no accounts registered. Use: belt add <name> <server> <version>",
			"[warn]".yellow()
		);
		return Ok(());
	}
	println!(
		"  {}",
		format!("{:<12}  {:<28}  {}", "NAME", "SERVER", "VER").bold()
	);
	println!("{}", "\u{2500}".repeat(49).dimmed());
	for a in &reg.asteroid {
		println!(
			"  {:<12}  {:<28}  u{}",
			a.name, a.server, a.version
		);
	}
	Ok(())
}

fn cmd_remove(name: &str) -> Result<(), String> {
	if name.is_empty()
		|| !name
			.bytes()
			.all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
	{
		return Err("invalid asteroid name".into());
	}
	let path = registry::registry_path()?;
	let mut reg = Registry::load(&path)?;
	if !reg.remove(name) {
		return Err(format!("'{name}' not found in registry"));
	}
	reg.save(&path)?;
	cache::remove(name)?;
	eprintln!("{} removed: {name}", "[ok]".green());
	Ok(())
}

fn cmd_status(refresh: bool, json: bool) -> Result<(), String> {
	let path = registry::registry_path()?;
	let reg = Registry::load(&path)?;
	if reg.asteroid.is_empty() {
		eprintln!(
			"{} no accounts registered. Use: belt add <name> <server> <version>",
			"[warn]".yellow()
		);
		return Ok(());
	}
	status::show_all(refresh, json, &reg.asteroid)
}

fn cmd_import(legacy_path: &str) -> Result<(), String> {
	let content =
		fs::read_to_string(legacy_path).map_err(|e| format!("failed to read {legacy_path}: {e}"))?;

	let reg_path = registry::registry_path()?;
	let mut reg = Registry::load(&reg_path)?;
	let mut count = 0;

	for line in content.lines() {
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}
		let parts: Vec<&str> = line.split_whitespace().collect();
		if parts.len() < 2 {
			eprintln!(
				"{} skipping malformed line: {line}",
				"[warn]".yellow()
			);
			continue;
		}
		let name = parts[0];
		let server = parts[1];
		let version: u8 = parts
			.get(2)
			.ok_or_else(|| format!("missing version in line: {line}"))?
			.parse()
			.map_err(|_| format!("invalid version in line: {line}"))?;
		registry::validate_asteroid(name, server, version)
			.map_err(|e| format!("invalid asteroid in line '{line}': {e}"))?;

		reg.add(Asteroid {
			name: name.to_string(),
			server: server.to_string(),
			version,
		});
		count += 1;

		// Try to import the corresponding cache file
		import_cache_file(legacy_path, name)?;
	}

	reg.save(&reg_path)?;
	eprintln!(
		"{} imported {count} asteroid(s) to {}",
		"[ok]".green(),
		reg_path.display()
	);
	Ok(())
}

/// Import a single cache file from the legacy format (KEY=VALUE) to TOML.
fn import_cache_file(legacy_list_path: &str, name: &str) -> Result<(), String> {
	use std::path::Path;

	let legacy_dir = Path::new(legacy_list_path)
		.parent()
		.ok_or_else(|| "cannot determine legacy directory".to_string())?;
	let cache_path = legacy_dir.join("cache").join(name);

	if !cache_path.exists() {
		return Ok(());
	}

	let content = fs::read_to_string(&cache_path)
		.map_err(|e| format!("failed to read cache for {name}: {e}"))?;

	let mut status = cache::CachedStatus::default();
	status.name = name.to_string();

	for line in content.lines() {
		let line = line.trim_end_matches('\r');
		if let Some(val) = line.strip_prefix("UPDATED=") {
			status.updated = val.to_string();
		} else if let Some(val) = line.strip_prefix("SERVER=") {
			status.server = val.to_string();
		} else if let Some(val) = line.strip_prefix("VERSION=") {
			status.version = val.parse().unwrap_or(7);
		} else if let Some(val) = line.strip_prefix("PORTS=") {
			status.ports = parse_legacy_csv(val);
		} else if let Some(val) = line.strip_prefix("WEB_DOMAINS=") {
			status.web_domains = parse_legacy_csv(val);
		} else if let Some(val) = line.strip_prefix("MAIL_DOMAINS=") {
			status.mail_domains = parse_legacy_csv(val);
		} else if let Some(val) = line.strip_prefix("MAIL_USERS=") {
			status.mail_users = parse_legacy_csv(val);
		}
	}

	cache::save(&status)?;
	Ok(())
}

fn parse_legacy_csv(s: &str) -> Vec<String> {
	if s.is_empty() {
		return Vec::new();
	}
	s.split(',')
		.map(|s| s.trim().to_string())
		.filter(|s| !s.is_empty() && s != "No mailboxes found.")
		.collect()
}

fn handle_passthrough(name: &str, rest: &[String]) -> Result<(), String> {
	let reg_path = registry::registry_path()?;
	let reg = Registry::load(&reg_path)?;
	let asteroid = reg
		.lookup(name)
		.ok_or_else(|| format!("'{name}' not found in registry"))?;

	// Special case: `belt <name> status`
	if rest.first().is_some_and(|s| s == "status") {
		let s = status::refresh_one(asteroid)?;
		println!();
		let age = "just now".to_string();
		status::print_status(&s, &age);
		return Ok(());
	}

	// Detect generation before selecting generation-specific command forms.
	let version = status::detect_version(asteroid)?;
	if version != asteroid.version {
		return Err(format!(
			"'{name}' is registered as U{}, but /etc/os-release reports U{}; update registry before retrying",
			asteroid.version, version
		));
	}
	let args: Vec<String> = rest.to_vec();
	let translated = translate::translate(version, &args)?;
	let code = ssh::passthrough(asteroid, &translated)?;
	if code != 0 {
		process::exit(code);
	}
	Ok(())
}
