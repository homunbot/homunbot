use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::registry::{
    get_optional_bool, get_optional_string, get_string_param, Tool, ToolContext, ToolResult,
};

pub struct CreateSkillTool;

impl CreateSkillTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for CreateSkillTool {
    fn name(&self) -> &str {
        "create_skill"
    }

    fn description(&self) -> &str {
        "Generate and install a new Homun skill from a natural-language request. \
         Creates SKILL.md plus a starter script, reuses nearby skill patterns when relevant, \
         validates the generated files, and installs the result into ~/.homun/skills."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Natural-language description of the skill to create."
                },
                "name": {
                    "type": "string",
                    "description": "Optional explicit skill name (kebab-case preferred)."
                },
                "language": {
                    "type": "string",
                    "enum": ["python", "bash", "javascript"],
                    "description": "Optional preferred starter script language."
                },
                "overwrite": {
                    "type": "boolean",
                    "description": "Replace an existing generated skill with the same name."
                }
            },
            "required": ["prompt"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let prompt = get_string_param(&args, "prompt")?;
        let request = crate::skills::creator::SkillCreationRequest {
            prompt,
            name: get_optional_string(&args, "name"),
            language: get_optional_string(&args, "language"),
            overwrite: get_optional_bool(&args, "overwrite").unwrap_or(false),
        };

        let result = crate::skills::creator::create_skill(request).await?;
        let reused = if result.reused_skills.is_empty() {
            "none".to_string()
        } else {
            result.reused_skills.join(", ")
        };

        Ok(ToolResult::success(format!(
            "Skill created.\n\
             name={}\n\
             path={}\n\
             script={}\n\
             language={}\n\
             reused_skills={}\n\
             smoke_test={}\n\
             security_risk={}/100\n\
             validation={}",
            result.name,
            result.path.display(),
            result.script_path.display(),
            result.script_language,
            reused,
            if result.smoke_test_passed {
                "passed"
            } else {
                "skipped"
            },
            result.security_report.risk_score,
            result.validation_notes.join(" | ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_skill_tool_schema() {
        let schema = CreateSkillTool::new().parameters();
        assert!(schema.get("properties").is_some());
        assert!(schema.get("required").is_some());
    }
}
