# Providers

## Purpose

This subsystem owns model-provider abstraction, provider selection, reliability wrappers, health tracking, and the fallback path used when native tool calling is weak.

## Primary Code

- `src/provider/mod.rs`
- `src/provider/traits.rs`
- `src/provider/factory.rs`
- `src/provider/reliable.rs`
- `src/provider/health.rs`
- `src/provider/capabilities.rs`
- `src/provider/xml_dispatcher.rs`
- `src/config/schema.rs`

## Provider Families In Code

The current runtime has three concrete provider implementations:

- `AnthropicProvider`
- `OllamaProvider`
- `OpenAICompatProvider`

Most of the provider catalog exposed in config/UI is routed through the OpenAI-compatible layer.

## Resolution Rules

`Config::resolve_provider()` decides which provider config to use for a model string. The resolution order is:

1. direct keyword match
2. explicit local/cloud prefixes such as `ollama/`, `ollama_cloud/`, `vllm/`, `custom/`
3. gateway providers such as OpenRouter or AiHubMix
4. fallback to the first configured provider

`Config::is_provider_configured()` checks both encrypted storage and config values.

## Reliability Layer

The runtime wraps provider access with reliability concerns:

- retries/fallback models
- provider health tracking
- "last good" behavior
- hot rebuild when model/provider config changes

The health tracker is also shared with the web UI.

## Tool Calling Compatibility

Not every provider/model pair is treated the same.

- native function calling is preferred when reliable
- XML tool dispatch is available as a compatibility fallback
- provider-specific and global config can force XML mode
- Ollama models are special-cased in auto-detection logic

This means a tool failure can come from either tool execution or model dispatch style, and both matter when debugging.

## Related Config

- `[agent]`
- `[agent.model_overrides]`
- `[providers.*]`
- provider-specific `force_xml_tools`

## Failure Modes And Limits

- broad provider catalog does not mean equal maturity across all providers
- capability mismatches can still surface at runtime despite config-level resolution
- some providers are only available through the OpenAI-compatible path, so feature parity depends on their backend behavior

## Change Checklist

Update this document when you change:

- provider resolution rules
- failover/reliability behavior
- XML dispatch heuristics
- supported provider catalog
