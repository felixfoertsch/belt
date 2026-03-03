/// Translates canonical (unified) CLI args to version-specific `uberspace` args.
///
/// The canonical form uses v7-style commands. For v8 asteroids, certain commands
/// are rewritten to the v8 equivalents. For v7, args pass through unchanged.
pub fn translate(version: u8, args: &[String]) -> Result<Vec<String>, String> {
	if version == 7 || args.is_empty() {
		return Ok(args.to_vec());
	}
	if version != 8 {
		return Err(format!("unsupported uberspace version: {version}"));
	}

	let joined = args.iter().map(|s| s.as_str()).collect::<Vec<_>>();

	match joined.as_slice() {
		// mail user list → mail address list
		["mail", "user", "list"] => Ok(words("mail address list")),

		// mail user add <x> → mail address add <x>
		["mail", "user", "add", addr] => Ok(words_with("mail address add", addr)),

		// mail user del <x> → mail address del <x>
		["mail", "user", "del", addr] => Ok(words_with("mail address del", addr)),

		// port list → web backend list
		["port", "list"] => Ok(words("web backend list")),

		// tools version use <tool> <ver> → tool version set <tool> <ver>
		["tools", "version", "use", tool, ver] => {
			Ok(vec![
				"tool".into(),
				"version".into(),
				"set".into(),
				(*tool).into(),
				(*ver).into(),
			])
		}

		// tools restart <tool> → web <tool> reload
		["tools", "restart", tool] => {
			Ok(vec!["web".into(), (*tool).into(), "reload".into()])
		}

		// web backend set <path> --http --port <n> → web backend add <path> port <n>
		["web", "backend", "set", path, "--http", "--port", port] => {
			Ok(vec![
				"web".into(),
				"backend".into(),
				"add".into(),
				(*path).into(),
				"port".into(),
				(*port).into(),
			])
		}

		// Everything else passes through unchanged
		_ => Ok(args.to_vec()),
	}
}

fn words(s: &str) -> Vec<String> {
	s.split_whitespace().map(String::from).collect()
}

fn words_with(prefix: &str, suffix: &str) -> Vec<String> {
	let mut v = words(prefix);
	v.push(suffix.to_string());
	v
}

#[cfg(test)]
mod tests {
	use super::*;

	fn args(s: &str) -> Vec<String> {
		s.split_whitespace().map(String::from).collect()
	}

	#[test]
	fn v7_passthrough() {
		let input = args("mail user list");
		assert_eq!(translate(7, &input).unwrap(), input);
	}

	#[test]
	fn v8_mail_user_list() {
		let result = translate(8, &args("mail user list")).unwrap();
		assert_eq!(result, args("mail address list"));
	}

	#[test]
	fn v8_mail_user_add() {
		let result = translate(8, &args("mail user add foo@bar.de")).unwrap();
		assert_eq!(result, args("mail address add foo@bar.de"));
	}

	#[test]
	fn v8_mail_user_del() {
		let result = translate(8, &args("mail user del foo@bar.de")).unwrap();
		assert_eq!(result, args("mail address del foo@bar.de"));
	}

	#[test]
	fn v8_port_list() {
		let result = translate(8, &args("port list")).unwrap();
		assert_eq!(result, args("web backend list"));
	}

	#[test]
	fn v8_tools_version_use() {
		let result = translate(8, &args("tools version use php 8.2")).unwrap();
		assert_eq!(result, args("tool version set php 8.2"));
	}

	#[test]
	fn v8_tools_restart() {
		let result = translate(8, &args("tools restart php")).unwrap();
		assert_eq!(result, args("web php reload"));
	}

	#[test]
	fn v8_web_backend_set() {
		let result = translate(8, &args("web backend set / --http --port 8080")).unwrap();
		assert_eq!(result, args("web backend add / port 8080"));
	}

	#[test]
	fn v8_unknown_passthrough() {
		let input = args("web domain list");
		assert_eq!(translate(8, &input).unwrap(), input);
	}

	#[test]
	fn empty_args() {
		let result = translate(8, &[]).unwrap();
		assert!(result.is_empty());
	}

	#[test]
	fn unsupported_version() {
		let result = translate(9, &args("port list"));
		assert!(result.is_err());
	}
}
