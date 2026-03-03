use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CachedStatus {
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

pub fn cache_dir() -> Result<PathBuf, String> {
	let config_dir =
		dirs::config_dir().ok_or_else(|| "cannot determine config directory".to_string())?;
	Ok(config_dir.join("uc").join("cache"))
}

pub fn save(status: &CachedStatus) -> Result<(), String> {
	let dir = cache_dir()?;
	fs::create_dir_all(&dir).map_err(|e| format!("failed to create cache directory: {e}"))?;
	let path = dir.join(format!("{}.toml", status.name));
	let content =
		toml::to_string_pretty(status).map_err(|e| format!("failed to serialize cache: {e}"))?;
	fs::write(&path, content).map_err(|e| format!("failed to write cache: {e}"))
}

pub fn load(name: &str) -> Result<Option<CachedStatus>, String> {
	let path = cache_dir()?.join(format!("{name}.toml"));
	if !path.exists() {
		return Ok(None);
	}
	let content = fs::read_to_string(&path).map_err(|e| format!("failed to read cache: {e}"))?;
	let status: CachedStatus =
		toml::from_str(&content).map_err(|e| format!("failed to parse cache: {e}"))?;
	Ok(Some(status))
}

pub fn load_all() -> Result<Vec<CachedStatus>, String> {
	let dir = cache_dir()?;
	if !dir.exists() {
		return Ok(Vec::new());
	}
	let mut results = Vec::new();
	let entries = fs::read_dir(&dir).map_err(|e| format!("failed to read cache directory: {e}"))?;
	for entry in entries {
		let entry = entry.map_err(|e| format!("failed to read cache entry: {e}"))?;
		let path = entry.path();
		if path.extension().and_then(|e| e.to_str()) == Some("toml") {
			let content =
				fs::read_to_string(&path).map_err(|e| format!("failed to read cache file: {e}"))?;
			match toml::from_str::<CachedStatus>(&content) {
				Ok(status) => results.push(status),
				Err(e) => eprintln!("warning: skipping {}: {e}", path.display()),
			}
		}
	}
	results.sort_by(|a, b| a.name.cmp(&b.name));
	Ok(results)
}

pub fn remove(name: &str) -> Result<(), String> {
	let path = cache_dir()?.join(format!("{name}.toml"));
	if path.exists() {
		fs::remove_file(&path).map_err(|e| format!("failed to remove cache: {e}"))?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn roundtrip_serialize() {
		let status = CachedStatus {
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
