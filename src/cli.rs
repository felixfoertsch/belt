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
#[command(
	name = "belt",
	about = "uberspace account manager",
	version,
	disable_version_flag = true
)]
struct Cli {
	/// Print version
	#[arg(short = 'v', long = "version", action = clap::ArgAction::Version)]
	version: Option<bool>,

	#[command(subcommand)]
	command: Option<Commands>,

	/// Asteroid name (for passthrough commands)
	#[arg(num_args = 1..)]
	args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Register asteroid(s)
	Add {
		/// Account name
        name: Option<String>,
		/// Server hostname (e.g. cetus.uberspace.de)
        server: Option<String>,
        /// Add every asteroid available from dashboard
        #[arg(short, long)]
        all: bool,
	},

    /// List local SSH inventory
	List,

    /// Log in to Uberspace dashboard
    Login {
        /// Mail address or username
        username: String,
        /// Read password from standard input instead of prompting
        #[arg(long)]
        password_stdin: bool,
	},

    /// Log out and remove local dashboard session
    Logout,

	/// Deregister an asteroid
	Remove {
		/// Account name
		name: String,
	},

	/// Show aggregate status from cache (or refresh all via SSH)
	Status {
        /// Show one asteroid only
        name: Option<String>,
        /// Refresh selected accounts via SSH before showing
		#[arg(long)]
		refresh: bool,
		/// Emit machine-readable JSON
		#[arg(long)]
		json: bool,
	},

    /// Import inventory from JSON or YAML
	Import {
        /// JSON or YAML inventory path
		path: String,
	},

    /// Export inventory as JSON or YAML
    Export {
        /// Write YAML instead of canonical JSON
        #[arg(long, conflicts_with = "json")]
        yaml: bool,
        /// Write canonical JSON
		#[arg(long)]
        json: bool,
	},
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
        Some(Commands::Add { name, server, all }) => {
            cmd_add(name.as_deref(), server.as_deref(), all)
        }
        Some(Commands::List) => cmd_list(),
        Some(Commands::Login {
            username,
            password_stdin,
        }) => dashboard::login(&username, password_stdin),
        Some(Commands::Logout) => dashboard::logout(),
		Some(Commands::Remove { name }) => cmd_remove(&name),
        Some(Commands::Status {
            name,
            refresh,
            json,
        }) => cmd_status(name.as_deref(), refresh, json),
		Some(Commands::Import { path }) => cmd_import(&path),
        Some(Commands::Export { yaml, json: _ }) => cmd_export(yaml),
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

fn cmd_add(name: Option<&str>, server: Option<&str>, all: bool) -> Result<(), String> {
    if all {
        if name.is_some() || server.is_some() {
            return Err("--all cannot be combined with name or server".into());
        }
        let dashboard_asteroids = dashboard::list()?;
	let path = registry::registry_path()?;
	let mut reg = Registry::load(&path)?;
        for dashboard_asteroid in &dashboard_asteroids {
            registry::validate_asteroid(
                &dashboard_asteroid.name,
                &dashboard_asteroid.hostname,
                dashboard_asteroid.version,
            )?;
            reg.add(Asteroid {
                name: dashboard_asteroid.name.clone(),
                server: dashboard_asteroid.hostname.clone(),
                version: dashboard_asteroid.version,
            });
        }
        reg.save(&path)?;
        eprintln!(
            "{} added {} asteroid(s)",
            "[ok]".green(),
            dashboard_asteroids.len()
        );
        return Ok(());
    }

    let name = name.ok_or_else(|| {
        "missing asteroid name; use `belt add <name> <server>` or `belt add --all`".to_string()
    })?;
    let server =
        server.ok_or_else(|| "missing server; use `belt add <name> <server>`".to_string())?;
    let probe = Asteroid {
		name: name.to_string(),
		server: server.to_string(),
        version: 7,
    };
    registry::validate_asteroid(name, server, 7)?;
    let version = status::detect_version(&probe)?;
    let path = registry::registry_path()?;
    let mut reg = Registry::load(&path)?;
    reg.add(Asteroid { version, ..probe });
	reg.save(&path)?;
	eprintln!(
        "{} registered: {name} @ {server} (u{version})",
		"[ok]".green()
	);
	Ok(())
}

fn cmd_list() -> Result<(), String> {
	let path = registry::registry_path()?;
	let reg = Registry::load(&path)?;
	if reg.asteroid.is_empty() {
		eprintln!(
            "{} no accounts registered. Use: belt add <name> <server>",
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
        println!("  {:<12}  {:<28}  u{}", a.name, a.server, a.version);
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

fn cmd_status(name: Option<&str>, refresh: bool, json: bool) -> Result<(), String> {
	let path = registry::registry_path()?;
	let reg = Registry::load(&path)?;
    if let Some(name) = name {
        let asteroid = reg
            .lookup(name)
            .ok_or_else(|| format!("'{name}' not found in inventory"))?;
        return status::show_all(refresh, json, std::slice::from_ref(asteroid));
    }
	if reg.asteroid.is_empty() {
		eprintln!(
            "{} no accounts registered. Use: belt add <name> <server>",
			"[warn]".yellow()
		);
		return Ok(());
	}
	status::show_all(refresh, json, &reg.asteroid)
}

fn cmd_import(path: &str) -> Result<(), String> {
    let content = fs::read_to_string(path).map_err(|e| format!("failed to read {path}: {e}"))?;
    let imported = if path.ends_with(".yaml") || path.ends_with(".yml") {
        Registry::from_yaml(&content)?
    } else {
        Registry::from_json(&content)?
    };
	let reg_path = registry::registry_path()?;
    let mut registry = Registry::load(&reg_path)?;
    let count = imported.asteroid.len();
    for asteroid in imported.asteroid {
        registry.add(asteroid);
		}
    registry.save(&reg_path)?;
	eprintln!(
		"{} imported {count} asteroid(s) to {}",
		"[ok]".green(),
		reg_path.display()
	);
	Ok(())
}

fn cmd_export(yaml: bool) -> Result<(), String> {
    let registry = Registry::load(&registry::registry_path()?)?;
    print!(
        "{}",
        if yaml {
            registry.to_yaml()?
        } else {
            registry.to_json()?
	}
    );
	Ok(())
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
