//! Discover literal environment dependencies without executing imported Skill code.
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EnvironmentVariable {
    pub name: String,
    pub secret: bool,
}

pub(crate) fn discover(package_path: &str, instructions: &str) -> Vec<EnvironmentVariable> {
    let mut names = BTreeSet::new();
    scan_source(instructions, &mut names);
    if !package_path.is_empty() {
        if let Some(root) = Path::new(package_path).parent() {
            scan_directory(root, &mut names, 0);
        }
    }
    names
        .into_iter()
        .map(|name| {
            let upper = name.to_ascii_uppercase();
            let secret = ["SECRET", "PASSWORD", "TOKEN", "API_KEY", "PRIVATE_KEY"]
                .iter()
                .any(|part| upper.contains(part));
            EnvironmentVariable { name, secret }
        })
        .collect()
}

fn scan_directory(root: &Path, names: &mut BTreeSet<String>, depth: usize) {
    if depth > 12 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().into_owned();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !["node_modules", ".git", ".venv", "__pycache__"].contains(&filename.as_str()) {
                scan_directory(&path, names, depth + 1);
            }
        } else if kind.is_file() {
            if entry
                .metadata()
                .map_or(true, |metadata| metadata.len() > 1_048_576)
            {
                continue;
            }
            let declaration = filename == ".env.example";
            let source = matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("py" | "js" | "ts" | "mjs" | "cjs" | "ps1" | "sh" | "md")
            );
            if !declaration && !source {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if declaration {
                for line in content.lines() {
                    let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
                    if let Some((name, _)) = line.split_once('=') {
                        let name = name.trim();
                        if valid_name(name) {
                            names.insert(name.to_owned());
                        }
                    }
                }
            } else {
                scan_source(&content, names);
            }
        }
    }
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn scan_source(source: &str, names: &mut BTreeSet<String>) {
    // Literal accesses only. Computed names must be listed in .env.example.
    static PATTERNS: std::sync::OnceLock<Vec<Regex>> = std::sync::OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        [
            r#"\b(?:os\.)?(?:getenv|environ\.get)\s*\(\s*["']([A-Za-z_][A-Za-z0-9_]*)["']"#,
            r#"\b(?:os\.environ|process\.env)\s*\[\s*["']([A-Za-z_][A-Za-z0-9_]*)["']\s*\]"#,
            r"\bprocess\.env\.([A-Za-z_][A-Za-z0-9_]*)",
            r"\$env:([A-Za-z_][A-Za-z0-9_]*)",
            r"\$\{([A-Z_][A-Z0-9_]*)\}",
        ]
        .iter()
        .map(|pattern| Regex::new(pattern).expect("static environment pattern"))
        .collect()
    });
    for pattern in patterns {
        for captures in pattern.captures_iter(source) {
            names.insert(captures[1].to_string());
        }
    }
}

pub(crate) fn validate_values(
    definitions: &[EnvironmentVariable],
    values: &BTreeMap<String, String>,
) -> Result<(), String> {
    for (name, value) in values {
        if !definitions
            .iter()
            .any(|definition| definition.name == *name)
        {
            return Err(format!("Skill 未声明环境变量：{name}"));
        }
        if value.contains('\0') || value.len() > 16384 {
            return Err(format!("环境变量 {name} 的值无效或过长"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_variables_from_package_not_skill_identity() {
        let root = std::env::temp_dir().join(format!("skill-env-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(
            root.join(".env.example"),
            "# configuration\nCUSTOM_ENDPOINT=\nexport ACCESS_TOKEN=\n",
        )
        .unwrap();
        std::fs::write(root.join("scripts/run.py"), "os.environ.get('CUSTOM_ENDPOINT')\nos.getenv(\"PYTHON_KEY\")\nos.environ['ANOTHER_VALUE']").unwrap();
        std::fs::write(
            root.join("scripts/run.ts"),
            "process.env.JS_URL; process.env['ACCESS_TOKEN'];",
        )
        .unwrap();
        let variables = discover(root.join("SKILL.md").to_str().unwrap(), "# Generic skill");
        assert_eq!(
            variables
                .iter()
                .map(|v| v.name.as_str())
                .collect::<Vec<_>>(),
            [
                "ACCESS_TOKEN",
                "ANOTHER_VALUE",
                "CUSTOM_ENDPOINT",
                "JS_URL",
                "PYTHON_KEY"
            ]
        );
        assert!(variables[0].secret);
        assert!(discover("", "# BOM quote without environment accesses").is_empty());
        assert!(
            validate_values(
                &variables,
                &BTreeMap::from([("UNKNOWN".into(), "x".into())])
            )
            .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
