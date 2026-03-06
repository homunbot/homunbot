use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct LegacySkillManifest {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub script_paths: Vec<String>,
    pub pip_dependencies: Vec<String>,
    pub npm_dependencies: Vec<String>,
    pub source_file: String,
}

#[derive(Debug, Clone)]
pub struct AdaptedSkill {
    pub name: String,
    pub description: String,
    pub generated_skill_md: bool,
    pub script_mappings: Vec<(String, String)>,
    pub notes: Vec<String>,
}

pub fn parse_legacy_manifest(source_file: &str, content: &str) -> Result<LegacySkillManifest> {
    let value = if source_file.ends_with(".json") {
        serde_json::from_str::<Value>(content)
            .with_context(|| format!("Invalid JSON in {source_file}"))?
    } else {
        serde_json::to_value(
            toml::from_str::<toml::Value>(content)
                .with_context(|| format!("Invalid TOML in {source_file}"))?,
        )
        .context("Failed to convert TOML manifest")?
    };

    let name = value_string(&value, &["name", "id", "slug"])
        .as_deref()
        .map(normalize_skill_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "adapted-skill".to_string());
    let description = value_string(
        &value,
        &[
            "description",
            "summary",
            "title",
            "display_name",
            "displayName",
        ],
    )
    .unwrap_or_else(|| "Adapted legacy skill".to_string());

    let script_paths = collect_script_paths(&value);
    let pip_dependencies = collect_string_values(
        &value,
        &[
            &["dependencies", "pip"],
            &["pip"],
            &["python", "dependencies"],
        ],
    );
    let npm_dependencies = collect_string_values(
        &value,
        &[
            &["dependencies", "npm"],
            &["npm"],
            &["node", "dependencies"],
        ],
    );

    Ok(LegacySkillManifest {
        name,
        description,
        license: value_string(&value, &["license"]),
        script_paths,
        pip_dependencies,
        npm_dependencies,
        source_file: source_file.to_string(),
    })
}

pub async fn adapt_legacy_skill_dir(skill_dir: &Path) -> Result<Option<AdaptedSkill>> {
    if skill_dir.join("SKILL.md").exists() {
        return Ok(None);
    }

    let mut manifest_path = None;
    for candidate in ["SKILL.toml", "manifest.json"] {
        let path = skill_dir.join(candidate);
        if path.exists() {
            manifest_path = Some(path);
            break;
        }
    }
    let Some(manifest_path) = manifest_path else {
        return Ok(None);
    };

    let source_file = manifest_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("manifest");
    let manifest_content = tokio::fs::read_to_string(&manifest_path)
        .await
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest = parse_legacy_manifest(source_file, &manifest_content)?;

    let scripts_dir = skill_dir.join("scripts");
    tokio::fs::create_dir_all(&scripts_dir)
        .await
        .with_context(|| format!("Failed to create {}", scripts_dir.display()))?;

    let mut script_mappings = Vec::new();
    let mut candidate_paths = manifest.script_paths.clone();
    if candidate_paths.is_empty() {
        candidate_paths = discover_script_candidates(skill_dir);
    }

    for relative in candidate_paths {
        let source = skill_dir.join(&relative);
        if !source.exists() || source.is_dir() {
            continue;
        }
        let Some(file_name) = source.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let destination = scripts_dir.join(file_name);
        if source != destination {
            tokio::fs::copy(&source, &destination)
                .await
                .with_context(|| {
                    format!(
                        "Failed to copy legacy script {} to {}",
                        source.display(),
                        destination.display()
                    )
                })?;
        }
        script_mappings.push((relative.replace('\\', "/"), format!("scripts/{file_name}")));
    }

    if !manifest.pip_dependencies.is_empty() && !skill_dir.join("requirements.txt").exists() {
        tokio::fs::write(
            skill_dir.join("requirements.txt"),
            manifest.pip_dependencies.join("\n") + "\n",
        )
        .await
        .ok();
    }

    let notes = build_adaptation_notes(&manifest, &script_mappings);
    let skill_md = build_adapted_skill_md(&manifest, &script_mappings, &notes);
    tokio::fs::write(skill_dir.join("SKILL.md"), skill_md)
        .await
        .with_context(|| format!("Failed to write {}", skill_dir.join("SKILL.md").display()))?;

    Ok(Some(AdaptedSkill {
        name: manifest.name,
        description: manifest.description,
        generated_skill_md: true,
        script_mappings,
        notes,
    }))
}

fn build_adaptation_notes(
    manifest: &LegacySkillManifest,
    script_mappings: &[(String, String)],
) -> Vec<String> {
    let mut notes = Vec::new();
    notes.push(format!(
        "Adapted from legacy manifest {}",
        manifest.source_file
    ));
    if script_mappings.is_empty() {
        notes.push("No script entrypoint was detected automatically".to_string());
    }
    if !manifest.npm_dependencies.is_empty() {
        notes.push(format!(
            "npm dependencies require manual review: {}",
            manifest.npm_dependencies.join(", ")
        ));
    }
    if !manifest.pip_dependencies.is_empty() {
        notes.push(format!(
            "pip dependencies captured in requirements.txt: {}",
            manifest.pip_dependencies.join(", ")
        ));
    }
    notes
}

fn build_adapted_skill_md(
    manifest: &LegacySkillManifest,
    script_mappings: &[(String, String)],
    notes: &[String],
) -> String {
    let script_lines = if script_mappings.is_empty() {
        "- No script mapping was inferred automatically.\n".to_string()
    } else {
        script_mappings
            .iter()
            .map(|(source, destination)| format!("- `{source}` -> `{destination}`"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    };
    let note_lines = notes
        .iter()
        .map(|note| format!("- {note}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"---
name: {}
description: {}
license: {}
compatibility: Adapted from legacy skill manifest
metadata:
  homun:
    adapted_from: {}
---

# {}

This skill was adapted automatically from a legacy manifest.

## Script Mapping

{}
## Notes

{}
"#,
        manifest.name,
        manifest.description,
        manifest.license.as_deref().unwrap_or("MIT"),
        manifest.source_file,
        title_case(&manifest.name),
        script_lines,
        note_lines
    )
}

fn collect_script_paths(value: &Value) -> Vec<String> {
    let mut paths = collect_string_values(
        value,
        &[
            &["entry"],
            &["entrypoint"],
            &["main"],
            &["script"],
            &["scripts"],
            &["files"],
        ],
    );
    paths.retain(|path| is_script_like(path));
    paths.sort();
    paths.dedup();
    paths
}

fn collect_string_values(value: &Value, paths: &[&[&str]]) -> Vec<String> {
    let mut out = Vec::new();
    for path in paths {
        if let Some(found) = value_path(value, path) {
            match found {
                Value::String(text) => out.push(text.to_string()),
                Value::Array(items) => out.extend(
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(ToString::to_string)),
                ),
                Value::Object(map) => out.extend(
                    map.values()
                        .filter_map(|item| item.as_str().map(ToString::to_string)),
                ),
                _ => {}
            }
        }
    }
    out
}

fn value_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|item| item.as_str()))
        .map(ToString::to_string)
}

fn value_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    Some(current)
}

fn discover_script_candidates(skill_dir: &Path) -> Vec<String> {
    let mut candidates = Vec::new();
    for root in ["src", "scripts"] {
        let dir = skill_dir.join(root);
        if !dir.exists() {
            continue;
        }
        collect_scripts_recursive(skill_dir, &dir, &mut candidates);
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn collect_scripts_recursive(skill_dir: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_scripts_recursive(skill_dir, &path, out);
            continue;
        }
        let relative = path
            .strip_prefix(skill_dir)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if is_script_like(&relative) {
            out.push(relative);
        }
    }
}

fn is_script_like(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default(),
        "py" | "sh" | "bash" | "zsh" | "js" | "mjs" | "cjs" | "ts" | "rb" | "pl"
    )
}

fn normalize_skill_name(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn title_case(value: &str) -> String {
    value
        .split('-')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_legacy_manifest_toml() {
        let manifest = parse_legacy_manifest(
            "SKILL.toml",
            r#"
name = "Price Tracker"
description = "Track prices"
entrypoint = "src/track.py"
[dependencies]
pip = ["requests", "pandas"]
"#,
        )
        .unwrap();

        assert_eq!(manifest.name, "price-tracker");
        assert_eq!(manifest.script_paths, vec!["src/track.py"]);
        assert_eq!(manifest.pip_dependencies.len(), 2);
    }

    #[tokio::test]
    async fn test_adapt_legacy_skill_dir_generates_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(dir.path().join("src"))
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join("SKILL.toml"),
            "name='legacy'\ndescription='Legacy skill'\nentry='src/run.py'\n",
        )
        .await
        .unwrap();
        tokio::fs::write(dir.path().join("src").join("run.py"), "print('ok')\n")
            .await
            .unwrap();

        let adapted = adapt_legacy_skill_dir(dir.path()).await.unwrap().unwrap();
        assert!(adapted.generated_skill_md);
        assert!(dir.path().join("SKILL.md").exists());
        assert!(dir.path().join("scripts").join("run.py").exists());
    }
}
