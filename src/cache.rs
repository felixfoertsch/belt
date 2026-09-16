use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const SCHEMA_VERSION: u8 = 1;

fn schema_version() -> u8 {
	SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachedStatus {
	#[serde(default = "schema_version")]
	pub schema_version: u8,
	pub updated: String,
	pub name: String,
	pub server: String,
	pub version: u8,
	#[serde(default)]
	pub ports: Vec<String>,
	#[serde(default)]
	pub web_domains: Vec<String>,
	#[serde(default)]
	pub mail_domains: Vec<String>,
	#[serde(default)]
	pub mail_users: Vec<String>,
}

impl Default for CachedStatus {
	fn default() -> Self {
		Self {
			schema_version: SCHEMA_VERSION,
			updated: String::new(),
			name: String::new(),
			server: String::new(),
			version: 0,
			ports: Vec::new(),
			web_domains: Vec::new(),
			mail_domains: Vec::new(),
			mail_users: Vec::new(),
		}
	}
}

pub fn cache_dir() -> Result<PathBuf, String> {
	let config_dir =
		dirs::config_dir().ok_or_else(|| "cannot determine config directory".to_string())?;
	Ok(config_dir.join("belt").join("cache"))
}

pub fn save(status: &CachedStatus) -> Result<(), String> {
	validate_name(&status.name)?;
	if status.schema_version != SCHEMA_VERSION {
		return Err(format!("unsupported cache schema version: {}", status.schema_version));
	}
	let dir = cache_dir()?;
	fs::create_dir_all(&dir).map_err(|e| format!("failed to create cache directory: {e}"))?;
	let path = dir.join(format!("{}.toml", status.name));
	let content =
		toml::to_string_pretty(status).map_err(|e| format!("failed to serialize cache: {e}"))?;
	let temp = dir.join(format!(".{}.{}.tmp", status.name, std::process::id()));
	fs::write(&temp, content).map_err(|e| format!("failed to write temporary cache: {e}"))?;
	fs::rename(&temp, &path).map_err(|e| format!("failed to replace cache atomically: {e}"))
}

pub fn load(name: &str) -> Result<Option<CachedStatus>, String> {
	validate_name(name)?;
	let path = cache_dir()?.join(format!("{name}.toml"));
	if !path.exists() {
		return Ok(None);
	}
	let content = fs::read_to_string(&path).map_err(|e| format!("failed to read cache: {e}"))?;
	let status: CachedStatus =
		toml::from_str(&content).map_err(|e| format!("failed to parse cache: {e}"))?;
	if status.schema_version != SCHEMA_VERSION {
		return Err(format!("unsupported cache schema version: {}", status.schema_version));
	}
	if status.name != name {
		return Err(format!("cache identity mismatch: expected {name}, got {}", status.name));
	}
	Ok(Some(status))
}

pub fn remove(name: &str) -> Result<(), String> {
	validate_name(name)?;
	let path = cache_dir()?.join(format!("{name}.toml"));
	if path.exists() {
		fs::remove_file(&path).map_err(|e| format!("failed to remove cache: {e}"))?;
	}
	Ok(())
}

fn validate_name(name: &str) -> Result<(), String> {
	if !name.is_empty()
		&& name
			.bytes()
			.all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
	{
		Ok(())
	} else {
		Err("invalid cache name".into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn roundtrip_serialize() {
		let status = CachedStatus {
			schema_version: 1,
			updated: "2026-02-25T17:31:06".into(),
			name: "danger".into(),
			server: "cetus.uberspace.de".into(),
			version: 7,
			ports: vec![],
			web_domains: vec!["danger.uber.space".into(), "rhqq2.de".into()],
			mail_domains: vec!["danger.uber.space".into()],
			mail_users: vec![],
		};
		let serialized = toml::to_string_pretty(&status).unwrap();
		let deserialized: CachedStatus = toml::from_str(&serialized).unwrap();
		assert_eq!(status, deserialized);
	}

	#[test]
	fn rejects_path_traversal_name() {
		assert!(validate_name("../../outside").is_err());
	}

	#[test]
	fn deserialize_empty_lists() {
		let toml_str = r#"
updated = "2026-01-01T00:00:00"
name = "test"
server = "test.uberspace.de"
version = 7
"#;
		let status: CachedStatus = toml::from_str(toml_str).unwrap();
		assert!(status.ports.is_empty());
		assert!(status.web_domains.is_empty());
	}
}
