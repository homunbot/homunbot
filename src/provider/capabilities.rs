use crate::config::ModelCapabilities;

pub fn detect_model_capabilities(provider_name: &str, model: &str) -> ModelCapabilities {
    let provider = provider_name.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();

    let image_input = match provider.as_str() {
        "anthropic" => {
            model.contains("claude-3")
                || model.contains("claude-sonnet-4")
                || model.contains("claude-opus-4")
                || model.contains("claude-haiku-4")
        }
        "ollama" | "ollama_cloud" => {
            model.contains("qwen3.5")
                || model.contains("qwen2.5-vl")
                || model.contains("qwen2.5vl")
                || model.contains("qwen-vl")
                || model.contains("llava")
                || model.contains("bakllava")
                || model.contains("llama3.2-vision")
                || model.contains("granite3.2-vision")
                || model.contains("minicpm-v")
                || model.contains("moondream")
                || model.contains("pixtral")
        }
        _ => {
            model.contains("gpt-4o")
                || model.contains("gpt-4.1")
                || model.contains("claude-3")
                || model.contains("claude-sonnet-4")
                || model.contains("claude-opus-4")
                || model.contains("claude-haiku-4")
                || model.contains("gemini")
                || model.contains("llava")
                || model.contains("qwen-vl")
                || model.contains("qvq")
                || model.contains("pixtral")
        }
    };

    let thinking = match provider.as_str() {
        "ollama" | "ollama_cloud" => {
            model.contains(":cloud")
                || model.contains("deepseek-r1")
                || model.contains("qwq")
                || model.contains("marco-o1")
        }
        "anthropic" => model.contains("claude-opus-4") || model.contains("claude-sonnet-4"),
        _ => {
            model.contains("deepseek-r1")
                || model.contains("deepseek-reasoner")
                || model.contains("qwq")
                || model.starts_with("o1")
                || model.contains("/o1")
                || model.starts_with("o3")
                || model.contains("/o3")
        }
    };

    let tool_calls = match provider.as_str() {
        // Ollama: default to true (most modern models support tools).
        // Blacklist only models known to NOT support native tool calling.
        "ollama" | "ollama_cloud" => {
            !(model.contains("deepseek-r1")
                || model.contains("deepseek-v3")
                || model.contains("phi-2")
                || model.contains("tinyllama")
                || model.contains("codellama")
                || model.contains("starcoder")
                || model.contains("stablelm")
                || model.contains("yi:"))
        }
        // All major cloud providers support tool calling natively
        _ => true,
    };

    ModelCapabilities {
        multimodal: image_input,
        image_input,
        tool_calls,
        thinking,
    }
}

pub fn supports_multimodal(provider_name: &str, model: &str) -> bool {
    detect_model_capabilities(provider_name, model).multimodal
}

pub fn supports_native_documents(_provider_name: &str, _model: &str) -> bool {
    false
}

pub fn supports_tool_calls(provider_name: &str, model: &str) -> bool {
    detect_model_capabilities(provider_name, model).tool_calls
}

pub fn supports_thinking(provider_name: &str, model: &str) -> bool {
    detect_model_capabilities(provider_name, model).thinking
}

#[cfg(test)]
mod tests {
    use super::{
        detect_model_capabilities, supports_multimodal, supports_native_documents,
        supports_thinking, supports_tool_calls,
    };

    #[test]
    fn detects_openai_family_models() {
        assert!(supports_multimodal("openai", "openai/gpt-4o"));
        assert!(supports_multimodal(
            "openrouter",
            "openrouter/google/gemini-2.0-flash"
        ));
        assert!(supports_multimodal("ollama", "ollama/qwen3.5:latest"));
        assert!(!supports_multimodal("ollama", "ollama/llama3"));
    }

    #[test]
    fn detects_tool_call_support() {
        // Ollama: default true for modern models
        assert!(supports_tool_calls("ollama", "ollama/qwen3.5:latest"));
        assert!(supports_tool_calls("ollama", "ollama/llama3:8b"));
        assert!(supports_tool_calls(
            "ollama_cloud",
            "ollama/nemotron-3-super:cloud"
        ));
        assert!(supports_tool_calls(
            "ollama_cloud",
            "ollama/devstral-2:cloud"
        ));
        // Models previously missing from whitelist now work
        assert!(supports_tool_calls("ollama_cloud", "ollama/glm-5:cloud"));
        assert!(supports_tool_calls(
            "ollama_cloud",
            "ollama/minimax-m2:cloud"
        ));
        assert!(supports_tool_calls("ollama_cloud", "ollama/glm-4.7-flash:cloud"));
        // Blacklisted models (no native tool support)
        assert!(!supports_tool_calls(
            "ollama_cloud",
            "ollama/deepseek-v3.2:cloud"
        ));
        assert!(!supports_tool_calls("ollama", "ollama/deepseek-r1:8b"));
        assert!(!supports_tool_calls("ollama", "ollama/phi-2:latest"));
        assert!(!supports_tool_calls("ollama", "ollama/tinyllama:latest"));
        // Cloud providers always support tool calls
        assert!(supports_tool_calls("openai", "openai/gpt-4o"));
        assert!(supports_tool_calls("anthropic", "anthropic/claude-sonnet-4"));
        assert!(detect_model_capabilities("openai", "openai/gpt-4o").image_input);
    }

    #[test]
    fn documents_are_disabled_until_provider_support_is_explicit() {
        assert!(!supports_native_documents("openai", "openai/gpt-4o"));
    }

    #[test]
    fn detects_thinking_models() {
        // Ollama thinking models
        assert!(supports_thinking("ollama", "ollama/deepseek-r1:8b"));
        assert!(supports_thinking("ollama", "ollama/qwq:32b"));
        assert!(supports_thinking("ollama", "ollama/qwen3:cloud"));
        assert!(supports_thinking("ollama", "ollama/marco-o1:7b"));
        assert!(!supports_thinking("ollama", "ollama/llama3:8b"));
        assert!(!supports_thinking("ollama", "ollama/qwen2.5:latest"));

        // Anthropic thinking models
        assert!(supports_thinking("anthropic", "anthropic/claude-opus-4"));
        assert!(supports_thinking(
            "anthropic",
            "anthropic/claude-sonnet-4-20250514"
        ));
        assert!(!supports_thinking("anthropic", "anthropic/claude-3-haiku"));

        // OpenAI / generic thinking models
        assert!(supports_thinking("openai", "o1-preview"));
        assert!(supports_thinking("openai", "o3-mini"));
        assert!(supports_thinking("openrouter", "deepseek/deepseek-r1"));
        assert!(supports_thinking(
            "openrouter",
            "deepseek/deepseek-reasoner"
        ));
        assert!(!supports_thinking("openai", "gpt-4o"));
    }
}
