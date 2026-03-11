use std::collections::{BTreeMap, HashMap};

use tokio::process::Command;

/// Minimal env allowlist used when command env sanitization is enabled.
pub const SAFE_ENV_KEYS: &[&str] = &[
    "PATH", "HOME", "USER", "SHELL", "LANG", "LC_ALL", "TERM", "TMPDIR",
];

pub(crate) const DOCKER_SAFE_ENV_KEYS: &[&str] = &[
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
    "DOCKER_TLS_VERIFY",
    "DOCKER_CERT_PATH",
    "DOCKER_CONFIG",
];

pub(crate) fn apply_sanitized_env(
    cmd: &mut Command,
    include_docker_keys: bool,
    extra_env: &HashMap<String, String>,
) {
    let env = resolved_sanitized_env(include_docker_keys, extra_env);
    apply_env_map(cmd, true, &env);
}

pub(crate) fn resolved_sanitized_env(
    include_docker_keys: bool,
    extra_env: &HashMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();

    for key in SAFE_ENV_KEYS {
        if let Ok(val) = std::env::var(key) {
            env.insert((*key).to_string(), val);
        }
    }
    if include_docker_keys {
        for key in DOCKER_SAFE_ENV_KEYS {
            if let Ok(val) = std::env::var(key) {
                env.insert((*key).to_string(), val);
            }
        }
    }
    env.entry("PATH".to_string())
        .or_insert_with(|| "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string());

    for (k, v) in extra_env {
        env.insert(k.clone(), v.clone());
    }

    env
}

pub(crate) fn resolved_full_env(extra_env: &HashMap<String, String>) -> BTreeMap<String, String> {
    let mut env = std::env::vars().collect::<BTreeMap<_, _>>();
    env.entry("PATH".to_string())
        .or_insert_with(|| "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string());
    for (k, v) in extra_env {
        env.insert(k.clone(), v.clone());
    }
    env
}

pub(crate) fn resolved_request_env(
    request: &super::types::SandboxExecutionRequest<'_>,
    include_docker_keys: bool,
) -> BTreeMap<String, String> {
    if request.sanitize_env {
        resolved_sanitized_env(include_docker_keys, request.extra_env)
    } else {
        resolved_full_env(request.extra_env)
    }
}

pub(crate) fn apply_env_map(cmd: &mut Command, clear: bool, env: &BTreeMap<String, String>) {
    if clear {
        cmd.env_clear();
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
}
