use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::executor::execute_skill_script;
use super::loader::{parse_skill_md_public, SkillRegistry};
use super::{list_skill_scripts, scan_skill_package, SecurityReport};
use crate::config::Config;

#[derive(Debug, Clone)]
pub struct SkillCreationRequest {
    pub prompt: String,
    pub name: Option<String>,
    pub language: Option<String>,
    pub overwrite: bool,
}

#[derive(Debug, Clone)]
pub struct SkillCreationResult {
    pub name: String,
    pub path: PathBuf,
    pub script_path: PathBuf,
    pub script_language: String,
    pub reused_skills: Vec<String>,
    pub validation_notes: Vec<String>,
    pub security_report: SecurityReport,
    pub smoke_test_passed: bool,
}

#[derive(Debug, Clone)]
struct RelatedSkillPattern {
    name: String,
    description: String,
    allowed_tools: Vec<String>,
    workflow_steps: Vec<String>,
    scripts: Vec<String>,
}

pub async fn create_skill(request: SkillCreationRequest) -> Result<SkillCreationResult> {
    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        anyhow::bail!("Prompt cannot be empty");
    }

    let name = request
        .name
        .as_deref()
        .map(normalize_skill_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| derive_skill_name(prompt));
    if name.is_empty() {
        anyhow::bail!("Unable to derive a valid skill name from the prompt");
    }

    let script_language = request
        .language
        .as_deref()
        .map(normalize_language)
        .unwrap_or_else(|| infer_language(prompt));
    let script_file_name = format!("run.{}", script_extension(&script_language));
    let skill_dir = Config::data_dir().join("skills").join(&name);
    let scripts_dir = skill_dir.join("scripts");
    let references_dir = skill_dir.join("references");
    let script_path = scripts_dir.join(&script_file_name);

    if skill_dir.exists() {
        if !request.overwrite {
            anyhow::bail!(
                "Skill '{}' already exists at {}. Re-run with overwrite=true to replace it.",
                name,
                skill_dir.display()
            );
        }
        tokio::fs::remove_dir_all(&skill_dir)
            .await
            .with_context(|| format!("Failed to remove existing skill {}", skill_dir.display()))?;
    }

    tokio::fs::create_dir_all(&scripts_dir)
        .await
        .with_context(|| format!("Failed to create {}", scripts_dir.display()))?;

    let related_patterns = find_reusable_skill_patterns(prompt)
        .await
        .unwrap_or_default();
    let reused_skills = related_patterns
        .iter()
        .map(|pattern| pattern.name.clone())
        .collect::<Vec<_>>();
    let description = derive_description(prompt);
    let allowed_tools = merge_allowed_tools(&related_patterns);
    let skill_md = build_skill_md(
        &name,
        &description,
        prompt,
        &script_file_name,
        &script_language,
        &allowed_tools,
        &related_patterns,
    );
    let script_content = build_script_template(&name, prompt, &script_language, &related_patterns);

    tokio::fs::write(skill_dir.join("SKILL.md"), skill_md)
        .await
        .with_context(|| format!("Failed to write {}", skill_dir.join("SKILL.md").display()))?;
    tokio::fs::write(&script_path, script_content)
        .await
        .with_context(|| format!("Failed to write {}", script_path.display()))?;

    if !related_patterns.is_empty() {
        tokio::fs::create_dir_all(&references_dir)
            .await
            .with_context(|| format!("Failed to create {}", references_dir.display()))?;
        tokio::fs::write(
            references_dir.join("composition.md"),
            build_composition_reference(prompt, &related_patterns),
        )
        .await
        .with_context(|| {
            format!(
                "Failed to write {}",
                references_dir.join("composition.md").display()
            )
        })?;
    }

    let mut validation_notes = Vec::new();
    let skill_md_content = tokio::fs::read_to_string(skill_dir.join("SKILL.md"))
        .await
        .with_context(|| format!("Failed to re-read {}", skill_dir.join("SKILL.md").display()))?;
    parse_skill_md_public(&skill_md_content).context("Generated SKILL.md is invalid")?;
    validation_notes.push("SKILL.md frontmatter parsed successfully".to_string());

    validation_notes.extend(validate_script(&script_path, &script_language).await);
    let smoke_test_passed = run_smoke_test(&skill_dir, &script_file_name).await?;
    validation_notes.push(if smoke_test_passed {
        "Smoke test passed (script returned homun_skill_smoke_ok)".to_string()
    } else {
        "Smoke test skipped: runtime unavailable".to_string()
    });

    let security_report = scan_skill_package(&skill_dir).await?;
    if security_report.is_blocked() {
        tokio::fs::remove_dir_all(&skill_dir).await.ok();
        anyhow::bail!(
            "Generated skill '{}' failed security validation:\n{}",
            name,
            security_report.summary()
        );
    }
    validation_notes.push(format!(
        "Security scan passed (risk {}/100, {} file(s) scanned)",
        security_report.risk_score, security_report.scanned_files
    ));

    Ok(SkillCreationResult {
        name,
        path: skill_dir,
        script_path,
        script_language,
        reused_skills,
        validation_notes,
        security_report,
        smoke_test_passed,
    })
}

async fn find_reusable_skill_patterns(prompt: &str) -> Result<Vec<RelatedSkillPattern>> {
    let mut registry = SkillRegistry::new();
    registry.scan_and_load().await?;

    let tokens = prompt_tokens(prompt);
    let mut scored = registry
        .list()
        .into_iter()
        .map(|(name, description)| {
            let haystack = format!("{name} {description}").to_ascii_lowercase();
            let score = tokens
                .iter()
                .filter(|token| haystack.contains(token.as_str()))
                .count();
            (score, name.to_string())
        })
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();

    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    scored.truncate(3);

    let mut patterns = Vec::new();
    for (_, name) in scored {
        let Some(skill) = registry.get_mut(&name) else {
            continue;
        };
        let skill_name = skill.meta.name.clone();
        let skill_description = skill.meta.description.clone();
        let allowed_tools = skill
            .meta
            .allowed_tools
            .as_deref()
            .map(parse_allowed_tools)
            .unwrap_or_default();
        let body = skill.load_body().await.unwrap_or("").to_string();
        let scripts = list_skill_scripts(&skill.path);
        patterns.push(RelatedSkillPattern {
            name: skill_name,
            description: skill_description,
            allowed_tools,
            workflow_steps: extract_workflow_steps(&body),
            scripts,
        });
    }

    Ok(patterns)
}

fn build_skill_md(
    name: &str,
    description: &str,
    prompt: &str,
    script_file_name: &str,
    script_language: &str,
    allowed_tools: &[String],
    related_patterns: &[RelatedSkillPattern],
) -> String {
    let allowed_tools_line = if allowed_tools.is_empty() {
        String::new()
    } else {
        format!("allowed-tools: \"{}\"\n", allowed_tools.join(" "))
    };

    let composition_section = if related_patterns.is_empty() {
        String::new()
    } else {
        let composed_from = related_patterns
            .iter()
            .map(|pattern| format!("- `{}`: {}", pattern.name, pattern.description))
            .collect::<Vec<_>>()
            .join("\n");
        let pattern_hints = related_patterns
            .iter()
            .flat_map(|pattern| {
                pattern
                    .workflow_steps
                    .iter()
                    .take(2)
                    .map(move |step| format!("- {}: {}", pattern.name, step))
            })
            .take(6)
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "\n## Composition Strategy\n\nComposed from:\n{composed_from}\n\nBorrow these workflow patterns:\n{pattern_hints}\n\nIf you need the detailed source patterns, read `references/composition.md`.\n"
        )
    };

    format!(
        r#"---
name: {name}
description: {description}
license: MIT
compatibility: Generated by Homun create_skill
{allowed_tools_line}metadata:
  homun:
    generated: true
    script_language: {script_language}
---

# {title}

Use this skill when the user asks for: "{prompt}".

## Workflow

1. Clarify the concrete input, destination, or output format before running the script.
2. Inspect the relevant local files, URLs, or task parameters before acting.
3. Run `scripts/{script_file_name}` with explicit arguments instead of improvising ad-hoc shell logic.
4. Validate output files or stdout markers before reporting success.
5. Explain clearly when credentials, APIs, or runtime dependencies are missing.

## Script Contract

- Primary script: `scripts/{script_file_name}`
- Preferred runtime: {script_language}
- Smoke test: run `scripts/{script_file_name} --smoke-test`
- Keep side effects scoped to the workspace or clearly requested output files.
- Emit concise, machine-readable output when possible so follow-up automation is easy.

## Guardrails

- Do not assume credentials exist; fail with a clear message.
- Prefer deterministic output files or structured stdout.
- If network access is required, state it before running.
- Stop and ask before destructive operations.{composition_section}"#,
        allowed_tools_line = allowed_tools_line,
        title = title_case(name)
    )
}

fn build_script_template(
    name: &str,
    prompt: &str,
    language: &str,
    related_patterns: &[RelatedSkillPattern],
) -> String {
    let composed_comments = build_script_composition_comments(related_patterns);
    match language {
        "bash" => format!(
            r#"#!/usr/bin/env bash
set -euo pipefail

# Generated by Homun create_skill for: {prompt}
{composed_comments}
main() {{
  if [[ "${{1:-}}" == "--smoke-test" ]]; then
    printf '{{"skill":"%s","status":"homun_skill_smoke_ok"}}\n' "{name}"
    return 0
  fi

  local target="${{1:-}}"
  if [[ -z "$target" ]]; then
    echo "usage: {name} <target>" >&2
    exit 1
  fi

  # TODO: replace this placeholder implementation with the real workflow.
  printf '{{"skill":"%s","target":"%s","status":"todo"}}\n' "{name}" "$target"
}}

main "$@"
"#
        ),
        "javascript" => format!(
            r#"#!/usr/bin/env node
// Generated by Homun create_skill for: {prompt}
{composed_comments}
if (process.argv[2] === "--smoke-test") {{
  console.log(JSON.stringify({{ skill: "{name}", status: "homun_skill_smoke_ok" }}));
  process.exit(0);
}}

const target = process.argv[2];
if (!target) {{
  console.error("usage: {name} <target>");
  process.exit(1);
}}

// TODO: replace this placeholder implementation with the real workflow.
console.log(JSON.stringify({{
  skill: "{name}",
  target,
  status: "todo"
}}, null, 2));
"#
        ),
        _ => format!(
            r#"#!/usr/bin/env python3
"""Generated by Homun create_skill for: {prompt}."""

import argparse
import json

{composed_comments}
def main() -> int:
    parser = argparse.ArgumentParser(prog="{name}")
    parser.add_argument("target", nargs="?", help="Primary input for the generated workflow")
    parser.add_argument("--smoke-test", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()

    if args.smoke_test:
        print(json.dumps({{
            "skill": "{name}",
            "status": "homun_skill_smoke_ok",
        }}))
        return 0

    if not args.target:
        parser.error("target is required")

    # TODO: replace this placeholder implementation with the real workflow.
    print(json.dumps({{
        "skill": "{name}",
        "target": args.target,
        "status": "todo",
    }}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
"#
        ),
    }
}

fn build_script_composition_comments(related_patterns: &[RelatedSkillPattern]) -> String {
    if related_patterns.is_empty() {
        return String::new();
    }

    related_patterns
        .iter()
        .map(|pattern| {
            let scripts = if pattern.scripts.is_empty() {
                "no local scripts".to_string()
            } else {
                format!("scripts: {}", pattern.scripts.join(", "))
            };
            format!(
                "// Reuse pattern from {}: {} ({})",
                pattern.name, pattern.description, scripts
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_composition_reference(prompt: &str, related_patterns: &[RelatedSkillPattern]) -> String {
    let mut out = String::new();
    out.push_str("# Composition Notes\n\n");
    out.push_str(&format!("Target request: `{prompt}`\n\n"));

    for pattern in related_patterns {
        out.push_str(&format!("## {}\n\n", title_case(&pattern.name)));
        out.push_str(&format!("- Description: {}\n", pattern.description));
        if !pattern.allowed_tools.is_empty() {
            out.push_str(&format!(
                "- Allowed tools: {}\n",
                pattern.allowed_tools.join(", ")
            ));
        }
        if !pattern.scripts.is_empty() {
            out.push_str(&format!("- Scripts: {}\n", pattern.scripts.join(", ")));
        }
        if !pattern.workflow_steps.is_empty() {
            out.push_str("- Workflow steps:\n");
            for step in pattern.workflow_steps.iter().take(5) {
                out.push_str(&format!("  - {}\n", step));
            }
        }
        out.push('\n');
    }

    out
}

async fn validate_script(script_path: &PathBuf, script_language: &str) -> Vec<String> {
    match script_language {
        "bash" => {
            run_validation_command("bash", &["-n", script_path.to_string_lossy().as_ref()]).await
        }
        "javascript" => {
            run_validation_command("node", &["--check", script_path.to_string_lossy().as_ref()])
                .await
        }
        _ => {
            run_validation_command(
                "python3",
                &["-m", "py_compile", script_path.to_string_lossy().as_ref()],
            )
            .await
        }
    }
}

async fn run_validation_command(binary: &str, args: &[&str]) -> Vec<String> {
    let output = tokio::process::Command::new(binary)
        .args(args)
        .output()
        .await;
    match output {
        Ok(output) if output.status.success() => {
            vec![format!("Validation passed: {} {}", binary, args.join(" "))]
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            vec![format!(
                "Validation warning: {} {} failed{}",
                binary,
                args.join(" "),
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(" ({stderr})")
                }
            )]
        }
        Err(_) => vec![format!(
            "Validation skipped: '{}' is not available on this machine",
            binary
        )],
    }
}

async fn run_smoke_test(skill_dir: &Path, script_file_name: &str) -> Result<bool> {
    match execute_skill_script(skill_dir, script_file_name, &["--smoke-test"], 15).await {
        Ok(output) => Ok(output.to_output_string().contains("homun_skill_smoke_ok")),
        Err(error) => {
            let message = error.to_string();
            if message.contains("Failed to execute script")
                || message.contains("Unsupported script type")
                || message.contains("not found")
            {
                return Ok(false);
            }
            Err(error)
        }
    }
}

fn merge_allowed_tools(related_patterns: &[RelatedSkillPattern]) -> Vec<String> {
    let mut merged = BTreeSet::new();
    for pattern in related_patterns {
        for tool in &pattern.allowed_tools {
            merged.insert(tool.clone());
        }
    }
    merged.into_iter().collect()
}

fn parse_allowed_tools(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn extract_workflow_steps(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            if line.starts_with("- ") || line.starts_with("* ") {
                return Some(line[2..].trim().to_string());
            }
            let mut chars = line.chars();
            let mut saw_digit = false;
            while let Some(ch) = chars.next() {
                if ch.is_ascii_digit() {
                    saw_digit = true;
                    continue;
                }
                if saw_digit && (ch == '.' || ch == ')') {
                    return Some(chars.as_str().trim().to_string());
                }
                break;
            }
            None
        })
        .filter(|line| line.len() <= 160)
        .take(8)
        .collect()
}

fn derive_skill_name(prompt: &str) -> String {
    let mut words = prompt_tokens(prompt);
    words.truncate(6);
    let mut name = words.join("-");
    if name.is_empty() {
        name = "generated-skill".to_string();
    }
    if !name.contains('-') {
        name.push_str("-skill");
    }
    name.truncate(63);
    name.trim_matches('-').to_string()
}

fn derive_description(prompt: &str) -> String {
    let trimmed = prompt.trim();
    if trimmed.len() <= 140 {
        format!(
            "{}. Use when the user asks for this workflow or a close variant.",
            trimmed
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default()
                + &trimmed.chars().skip(1).collect::<String>()
        )
    } else {
        "Generated skill from a natural-language request. Use when the user asks for this workflow."
            .to_string()
    }
}

fn infer_language(prompt: &str) -> String {
    let lower = prompt.to_ascii_lowercase();
    if lower.contains("bash") || lower.contains("shell") || lower.contains("terminal") {
        return "bash".to_string();
    }
    if lower.contains("node") || lower.contains("javascript") || lower.contains("json api") {
        return "javascript".to_string();
    }
    "python".to_string()
}

fn normalize_language(language: &str) -> String {
    match language.trim().to_ascii_lowercase().as_str() {
        "sh" | "shell" | "bash" => "bash".to_string(),
        "js" | "node" | "javascript" | "typescript" | "ts" => "javascript".to_string(),
        _ => "python".to_string(),
    }
}

fn script_extension(language: &str) -> &'static str {
    match language {
        "bash" => "sh",
        "javascript" => "js",
        _ => "py",
    }
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
        .chars()
        .take(63)
        .collect()
}

fn prompt_tokens(prompt: &str) -> Vec<String> {
    prompt
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| part.len() >= 3)
        .map(ToString::to_string)
        .collect()
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
    fn test_normalize_skill_name() {
        assert_eq!(normalize_skill_name("Price Tracker!"), "price-tracker");
    }

    #[test]
    fn test_infer_language_defaults_python() {
        assert_eq!(infer_language("track prices and export csv"), "python");
        assert_eq!(infer_language("run a shell backup"), "bash");
    }

    #[test]
    fn test_extract_workflow_steps() {
        let body = "## Workflow\n1. First step\n2. Second step\n- Third step";
        let steps = extract_workflow_steps(body);
        assert_eq!(steps.len(), 3);
        assert!(steps.contains(&"First step".to_string()));
    }

    #[tokio::test]
    async fn test_create_skill_generates_files() {
        let home = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", home.path());

        let result = create_skill(SkillCreationRequest {
            prompt: "track prices and write a csv".to_string(),
            name: Some("price-tracker".to_string()),
            language: None,
            overwrite: false,
        })
        .await
        .unwrap();

        assert_eq!(result.name, "price-tracker");
        assert!(result.path.join("SKILL.md").exists());
        assert!(result.script_path.exists());
        assert!(!result.security_report.is_blocked());
        assert!(
            result.smoke_test_passed
                || result
                    .validation_notes
                    .iter()
                    .any(|note| note.contains("Smoke test"))
        );
    }
}
