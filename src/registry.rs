use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Asteroid {
	pub name: String,
	pub server: String,
	pub version: u8,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Registry {
	#[serde(default)]
	pub asteroid: Vec<Asteroid>,
}

impl Registry {
	pub fn load(path: &PathBuf) -> Result<Self, String> {
		if !path.exists() {
			return Ok(Registry::default());
		}
		let content =
			fs::read_to_string(path).map_err(|e| format!("failed to read registry: {e}"))?;
		toml::from_str(&content).map_err(|e| format!("failed to parse registry: {e}"))
	}

	pub fn save(&self, path: &PathBuf) -> Result<(), String> {
		let parent = path
			.parent()
			.ok_or_else(|| "registry path has no parent directory".to_string())?;
		fs::create_dir_all(parent)
			.map_err(|e| format!("failed to create config directory: {e}"))?;
		let content =
			toml::to_string_pretty(self).map_err(|e| format!("failed to serialize registry: {e}"))?;
		atomic_write(path, content.as_bytes())
	}

	pub fn add(&mut self, asteroid: Asteroid) {
		self.asteroid.retain(|a| a.name != asteroid.name);
		self.asteroid.push(asteroid);
	}

	pub fn remove(&mut self, name: &str) -> bool {
		let before = self.asteroid.len();
		self.asteroid.retain(|a| a.name != name);
		self.asteroid.len() < before
	}

	pub fn lookup(&self, name: &str) -> Option<&Asteroid> {
		self.asteroid.iter().find(|a| a.name == name)
	}
}

pub fn registry_path() -> Result<PathBuf, String> {
	let config_dir =
		dirs::config_dir().ok_or_else(|| "cannot determine config directory".to_string())?;
	let belt_path = config_dir.join("belt").join("registry.toml");
	let legacy_path = config_dir.join("uc").join("registry.toml");
	migrate_legacy_registry(&legacy_path, &belt_path)?;
	Ok(belt_path)
}

pub fn validate_asteroid(name: &str, server: &str, version: u8) -> Result<(), String> {
	if name.is_empty()
		|| !name
			.bytes()
			.all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
	{
		return Err("name must contain only ASCII letters, digits, '-' or '_'".into());
	}
	if server.is_empty()
		|| server.len() > 253
		|| server.split('.').any(|label| {
			label.is_empty()
				|| label.len() > 63
				|| label.starts_with('-')
				|| label.ends_with('-')
				|| !label
					.bytes()
					.all(|c| c.is_ascii_alphanumeric() || c == b'-')
		})
	{
		return Err("server must be a valid DNS hostname".into());
	}
	if version != 7 && version != 8 {
		return Err(format!("version must be 7 or 8, got {version}"));
	}
	Ok(())
}

fn migrate_legacy_registry(legacy_path: &Path, belt_path: &Path) -> Result<(), String> {
	if belt_path.exists() || !legacy_path.exists() {
		return Ok(());
	}
	let content = fs::read(legacy_path).map_err(|e| format!("failed to read legacy registry: {e}"))?;
	atomic_write(belt_path, &content)
		.map_err(|e| format!("failed to migrate legacy registry: {e}"))
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
	let parent = path
		.parent()
		.ok_or_else(|| "path has no parent directory".to_string())?;
	fs::create_dir_all(parent).map_err(|e| format!("failed to create directory: {e}"))?;
	let temp_path = parent.join(format!(
		".{}.{}.tmp",
		path.file_name().and_then(|name| name.to_str()).unwrap_or("belt"),
		std::process::id()
	));
	fs::write(&temp_path, content).map_err(|e| format!("failed to write temporary file: {e}"))?;
	fs::rename(&temp_path, path).map_err(|e| format!("failed to replace file atomically: {e}"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;
	use tempfile::NamedTempFile;

	fn temp_path() -> (NamedTempFile, PathBuf) {
		let f = NamedTempFile::new().unwrap();
		let p = f.path().to_path_buf();
		(f, p)
	}

	#[test]
	fn add_and_lookup() {
		let mut reg = Registry::default();
		reg.add(Asteroid {
			name: "danger".into(),
			server: "cetus.uberspace.de".into(),
			version: 7,
		});
		let a = reg.lookup("danger").unwrap();
		assert_eq!(a.server, "cetus.uberspace.de");
		assert_eq!(a.version, 7);
	}

	#[test]
	fn add_replaces_existing() {
		let mut reg = Registry::default();
		reg.add(Asteroid {
			name: "danger".into(),
			server: "old.uberspace.de".into(),
			version: 7,
		});
		reg.add(Asteroid {
			name: "danger".into(),
			server: "new.uberspace.de".into(),
			version: 8,
		});
		assert_eq!(reg.asteroid.len(), 1);
		assert_eq!(reg.lookup("danger").unwrap().server, "new.uberspace.de");
	}

	#[test]
	fn remove_existing() {
		let mut reg = Registry::default();
		reg.add(Asteroid {
			name: "danger".into(),
			server: "cetus.uberspace.de".into(),
			version: 7,
		});
		assert!(reg.remove("danger"));
		assert!(reg.lookup("danger").is_none());
	}

	#[test]
	fn remove_nonexistent() {
		let mut reg = Registry::default();
		assert!(!reg.remove("nope"));
	}

	#[test]
	fn roundtrip_save_load() {
		let (_f, path) = temp_path();
		let mut reg = Registry::default();
		reg.add(Asteroid {
			name: "danger".into(),
			server: "cetus.uberspace.de".into(),
			version: 7,
		});
		reg.add(Asteroid {
			name: "impstr".into(),
			server: "pandora.uberspace.de".into(),
			version: 8,
		});
		reg.save(&path).unwrap();
		let loaded = Registry::load(&path).unwrap();
		assert_eq!(loaded.asteroid.len(), 2);
		assert_eq!(loaded.lookup("danger").unwrap().version, 7);
		assert_eq!(loaded.lookup("impstr").unwrap().version, 8);
	}

	#[test]
	fn load_nonexistent_returns_empty() {
		let path = PathBuf::from("/tmp/belt-test-nonexistent-registry.toml");
		let reg = Registry::load(&path).unwrap();
		assert!(reg.asteroid.is_empty());
	}

	#[test]
	fn validates_asteroid_fields() {
		assert!(validate_asteroid("danger", "cetus.uberspace.de", 7).is_ok());
		assert!(validate_asteroid("../danger", "cetus.uberspace.de", 7).is_err());
		assert!(validate_asteroid("danger", "bad host", 7).is_err());
		assert!(validate_asteroid("danger", "cetus.uberspace.de", 9).is_err());
	}

	#[test]
	fn migrates_legacy_registry_without_overwriting_belt() {
		let dir = tempfile::tempdir().unwrap();
		let legacy = dir.path().join("uc/registry.toml");
		let belt = dir.path().join("belt/registry.toml");
		fs::create_dir_all(legacy.parent().unwrap()).unwrap();
		fs::write(&legacy, "legacy").unwrap();
		migrate_legacy_registry(&legacy, &belt).unwrap();
		assert_eq!(fs::read_to_string(&belt).unwrap(), "legacy");
		fs::write(&legacy, "changed").unwrap();
		migrate_legacy_registry(&legacy, &belt).unwrap();
		assert_eq!(fs::read_to_string(&belt).unwrap(), "legacy");
	}

	#[test]
	fn parse_toml_format() {
		let toml_str = r#"
[[asteroid]]
name = "danger"
server = "cetus.uberspace.de"
version = 7

[[asteroid]]
name = "impstr"
server = "pandora.uberspace.de"
version = 8
"#;
		let (_f, path) = temp_path();
		let mut file = fs::File::create(&path).unwrap();
		file.write_all(toml_str.as_bytes()).unwrap();
		let reg = Registry::load(&path).unwrap();
		assert_eq!(reg.asteroid.len(), 2);
	}
}
