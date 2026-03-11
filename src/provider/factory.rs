//! Provider factory — creates LLM providers from config.
//!
//! Extracted from main.rs so that both startup and hot-reload code paths
//! can build provider chains without duplicating logic.

use std::sync::Arc;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::storage;

use super::health::ProviderHealthTracker;
use super::traits::Provider;
use super::{AnthropicProvider, OllamaProvider, OpenAICompatProvider, ReliableProvider};

/// Create the LLM provider from config, wrapped in a ReliableProvider
/// with retry and failover support.
///
/// Builds a provider chain: primary model + any `fallback_models` from config.
/// Each provider gets retry on transient errors (429, 5xx) and automatic
/// failover to the next provider on non-transient errors (401, 403).
pub fn create_provider(config: &Config) -> Result<Arc<dyn Provider>> {
    create_provider_for_model_with_fallbacks(config, &config.agent.model, true, None)
}

/// Create provider with an attached health tracker for circuit breaker support.
pub fn create_provider_with_health(
    config: &Config,
    tracker: Arc<ProviderHealthTracker>,
) -> Result<Arc<dyn Provider>> {
    create_provider_for_model_with_fallbacks(config, &config.agent.model, true, Some(tracker))
}

pub fn create_provider_for_model(
    config: &Config,
    primary_model: &str,
) -> Result<Arc<dyn Provider>> {
    create_provider_for_model_with_fallbacks(config, primary_model, true, None)
}

pub fn create_provider_for_model_without_fallbacks(
    config: &Config,
    primary_model: &str,
) -> Result<Arc<dyn Provider>> {
    create_provider_for_model_with_fallbacks(config, primary_model, false, None)
}

fn create_provider_for_model_with_fallbacks(
    config: &Config,
    primary_model: &str,
    include_fallbacks: bool,
    health_tracker: Option<Arc<ProviderHealthTracker>>,
) -> Result<Arc<dyn Provider>> {
    let (primary_name, primary) = create_single_provider(config, primary_model)
        .context("No provider configured. Add an API key to ~/.homun/config.toml")?;

    let mut chain = vec![(primary_name, primary, primary_model.to_string())];

    // Add fallback providers from config
    if include_fallbacks {
        for fallback_model in &config.agent.fallback_models {
            match create_single_provider(config, fallback_model) {
                Ok((name, provider)) => {
                    tracing::info!(
                        provider = %name,
                        model = %fallback_model,
                        "Registered fallback provider"
                    );
                    chain.push((name, provider, fallback_model.clone()));
                }
                Err(e) => {
                    tracing::warn!(
                        model = %fallback_model,
                        error = %e,
                        "Skipping fallback model — provider not configured"
                    );
                }
            }
        }
    }

    tracing::info!(
        primary_model = %primary_model,
        chain_length = chain.len(),
        include_fallbacks,
        "Provider chain ready"
    );

    let reliable = ReliableProvider::new(chain, crate::utils::retry::RetryConfig::default());
    let reliable = if let Some(tracker) = health_tracker {
        reliable.with_health(tracker)
    } else {
        reliable
    };
    Ok(Arc::new(reliable))
}

/// Create a single LLM provider instance for a given model string.
///
/// Resolves the provider from config, retrieves the API key (from encrypted
/// storage or plaintext with auto-migration), and constructs the appropriate
/// provider type (Anthropic, Ollama, or OpenAI-compatible).
pub fn create_single_provider(config: &Config, model: &str) -> Result<(String, Arc<dyn Provider>)> {
    let (provider_name, provider_config) = config
        .resolve_provider(model)
        .with_context(|| format!("No provider configured for model '{}'", model))?;

    tracing::info!(
        provider = provider_name,
        model = model,
        "Creating LLM provider"
    );

    // Get API key from secure storage (encrypted)
    let api_key = if provider_config.api_key == "***ENCRYPTED***" {
        let secrets = storage::global_secrets().context("Failed to access secure storage")?;
        let secret_key = storage::SecretKey::provider_api_key(provider_name);
        secrets.get(&secret_key)?.unwrap_or_default()
    } else if provider_config.api_key.is_empty() {
        String::new()
    } else {
        // Legacy: key stored in plaintext config — auto-migrate to encrypted storage
        tracing::warn!(
            provider = provider_name,
            "API key for '{}' is in plaintext config.toml — auto-migrating to encrypted storage",
            provider_name
        );
        let plaintext_key = provider_config.api_key.clone();
        if let Ok(secrets) = storage::global_secrets() {
            let secret_key = storage::SecretKey::provider_api_key(provider_name);
            if secrets.set(&secret_key, &plaintext_key).is_ok() {
                let mut migrated = config.clone();
                if let Some(pc) = migrated.providers.get_mut(provider_name) {
                    pc.api_key = "***ENCRYPTED***".to_string();
                    if let Err(e) = migrated.save() {
                        tracing::warn!(error = %e, "Failed to save migrated config");
                    } else {
                        tracing::info!(
                            provider = provider_name,
                            "Auto-migrated API key to encrypted storage"
                        );
                    }
                }
            }
        }
        plaintext_key
    };

    let name = provider_name.to_string();

    if provider_name == "anthropic" {
        let provider = AnthropicProvider::new(
            &api_key,
            provider_config.api_base.as_deref(),
            provider_config.extra_headers.clone(),
        );
        Ok((name, Arc::new(provider)))
    } else if provider_name == "ollama" || provider_name == "ollama_cloud" {
        let provider = OllamaProvider::new(&api_key, provider_config.api_base.as_deref());
        Ok((name, Arc::new(provider)))
    } else {
        let provider = OpenAICompatProvider::from_config(
            provider_name,
            &api_key,
            provider_config.api_base.as_deref(),
            provider_config.extra_headers.clone(),
        );
        Ok((name, Arc::new(provider)))
    }
}
