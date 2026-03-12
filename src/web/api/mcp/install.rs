use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::Json;
use serde::{Deserialize, Serialize};

use crate::web::server::AppState;

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct McpInstallGuideEnvSpec {
    key: String,
    description: Option<String>,
    required: Option<bool>,
    secret: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct McpInstallGuideRequest {
    id: String,
    display_name: Option<String>,
    description: Option<String>,
    docs_url: Option<String>,
    transport: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    env: Option<Vec<McpInstallGuideEnvSpec>>,
    language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpInstallGuideEnvHelp {
    key: String,
    why: String,
    where_to_get: String,
    format_hint: String,
    vault_hint: String,
    #[serde(default)]
    retrieval_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct McpInstallGuideDocumentation {
    url: Option<String>,
    summary: String,
    #[serde(default)]
    highlights: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct McpInstallGuideResponse {
    ok: bool,
    source: String, // llm | docs | fallback
    summary: String,
    steps: Vec<String>,
    env_help: Vec<McpInstallGuideEnvHelp>,
    notes: Vec<String>,
    documentation: Option<McpInstallGuideDocumentation>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct McpInstallGuideLlmParsed {
    summary: String,
    #[serde(default)]
    steps: Vec<String>,
    #[serde(default)]
    env_help: Vec<McpInstallGuideEnvHelp>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct McpInstallGuideDocsContext {
    url: Option<String>,
    summary: String,
    highlights: Vec<String>,
    text_excerpt: String,
}

#[derive(Clone, Copy)]
enum GuideLanguage {
    English,
    Italian,
}

impl GuideLanguage {
    fn from_request(value: Option<&str>) -> Self {
        match value.unwrap_or("en").trim().to_ascii_lowercase().as_str() {
            "it" | "it-it" | "italian" | "italiano" => Self::Italian,
            _ => Self::English,
        }
    }

    fn llm_label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Italian => "Italian",
        }
    }

    fn is_italian(self) -> bool {
        matches!(self, Self::Italian)
    }
}

// ── Text extraction helpers ──────────────────────────────────────

fn extract_json_object_block(input: &str) -> Option<&str> {
    let start = input.find('{')?;
    let end = input.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&input[start..=end])
}

fn strip_html_tags_for_docs(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if in_script || in_style {
            if chars[i] == '<' && i + 1 < chars.len() && chars[i + 1] == '/' {
                let rest: String = chars[i..].iter().take(20).collect();
                let rest_lower = rest.to_ascii_lowercase();
                if in_script && rest_lower.starts_with("</script") {
                    in_script = false;
                } else if in_style && rest_lower.starts_with("</style") {
                    in_style = false;
                }
            }
            i += 1;
            continue;
        }

        if chars[i] == '<' {
            let rest: String = chars[i..].iter().take(20).collect();
            let rest_lower = rest.to_ascii_lowercase();
            if rest_lower.starts_with("<script") {
                in_script = true;
            } else if rest_lower.starts_with("<style") {
                in_style = true;
            }
            in_tag = true;
            i += 1;
            continue;
        }

        if chars[i] == '>' && in_tag {
            in_tag = false;
            result.push('\n');
            i += 1;
            continue;
        }

        if !in_tag {
            result.push(chars[i]);
        }
        i += 1;
    }

    result
}

fn normalize_docs_text(input: &str) -> String {
    input
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn split_docs_fragments(input: &str) -> Vec<String> {
    let normalized = normalize_docs_text(input);
    let mut out = Vec::new();
    for line in normalized.lines() {
        for chunk in line.split(['.', ';']) {
            let trimmed = chunk.trim();
            if trimmed.len() < 12 || trimmed.len() > 260 {
                continue;
            }
            out.push(trimmed.to_string());
        }
    }
    out
}

fn docs_search_terms(spec: &McpInstallGuideEnvSpec) -> Vec<String> {
    let key = spec.key.to_ascii_lowercase();
    let mut terms = vec![key.clone(), key.replace('_', " ")];

    if key.contains("token") {
        terms.push("token".to_string());
    }
    if key.contains("api_key") || key.contains("apikey") {
        terms.push("api key".to_string());
    }
    if key.contains("client_id") {
        terms.push("client id".to_string());
    }
    if key.contains("client_secret") {
        terms.push("client secret".to_string());
    }
    if key.contains("refresh_token") {
        terms.push("refresh token".to_string());
    }
    if key.contains("access_token") {
        terms.push("access token".to_string());
    }
    if key.contains("authorization") {
        terms.push("authorization".to_string());
        terms.push("bearer".to_string());
    }

    terms.sort();
    terms.dedup();
    terms
}

fn find_relevant_doc_fragments(
    docs: &McpInstallGuideDocsContext,
    spec: Option<&McpInstallGuideEnvSpec>,
    limit: usize,
) -> Vec<String> {
    let mut terms = vec![
        "install".to_string(),
        "setup".to_string(),
        "authentication".to_string(),
        "oauth".to_string(),
        "environment variable".to_string(),
        "configuration".to_string(),
    ];
    if let Some(spec) = spec {
        terms.extend(docs_search_terms(spec));
    }

    let mut results = Vec::new();
    for fragment in docs.text_excerpt.lines() {
        let lower = fragment.to_ascii_lowercase();
        if terms.iter().any(|term| lower.contains(term)) {
            let value = fragment.trim();
            if !value.is_empty() && !results.iter().any(|r: &String| r == value) {
                results.push(value.to_string());
            }
        }
        if results.len() >= limit {
            break;
        }
    }
    results
}

fn docs_context_to_view(docs: &McpInstallGuideDocsContext) -> Option<McpInstallGuideDocumentation> {
    if docs.summary.trim().is_empty() && docs.highlights.is_empty() {
        return None;
    }
    Some(McpInstallGuideDocumentation {
        url: docs.url.clone(),
        summary: docs.summary.clone(),
        highlights: docs.highlights.clone(),
    })
}

// ── Documentation fetching ───────────────────────────────────────

fn parse_github_repo(url: &str) -> Option<(String, String)> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if parsed.domain()? != "github.com" {
        return None;
    }
    let mut segs = parsed
        .path_segments()?
        .filter(|seg| !seg.trim().is_empty())
        .take(2);
    let owner = segs.next()?.to_string();
    let repo = segs.next()?.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

async fn fetch_docs_text(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent("homun")
        .timeout(Duration::from_secs(12))
        .build()
        .ok()?;

    if let Some((owner, repo)) = parse_github_repo(url) {
        let api_url = format!("https://api.github.com/repos/{owner}/{repo}/readme");
        if let Ok(resp) = client
            .get(&api_url)
            .header("Accept", "application/vnd.github.raw")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    let normalized = normalize_docs_text(&text);
                    if !normalized.is_empty() {
                        return Some(normalized);
                    }
                }
            }
        }
    }

    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let raw = resp.text().await.ok()?;
    let stripped = if raw.contains("<html") || raw.contains("<HTML") {
        strip_html_tags_for_docs(&raw)
    } else {
        raw
    };
    let normalized = normalize_docs_text(&stripped);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

async fn fetch_install_docs_context(
    req: &McpInstallGuideRequest,
) -> Option<McpInstallGuideDocsContext> {
    let url = req.docs_url.as_deref()?.trim();
    if url.is_empty() {
        return None;
    }

    let text = fetch_docs_text(url).await?;
    let fragments = split_docs_fragments(&text);
    if fragments.is_empty() {
        return None;
    }

    let mut highlights = Vec::new();
    let keywords = [
        "install",
        "setup",
        "oauth",
        "authentication",
        "environment variable",
        "token",
        "api key",
        "client id",
        "client secret",
        "refresh token",
        "authorization",
        "bearer",
    ];
    for fragment in &fragments {
        let lower = fragment.to_ascii_lowercase();
        if keywords.iter().any(|keyword| lower.contains(keyword))
            && !highlights.iter().any(|line| line == fragment)
        {
            highlights.push(fragment.clone());
        }
        if highlights.len() >= 6 {
            break;
        }
    }

    let summary = if highlights.is_empty() {
        fragments
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(". ")
    } else {
        highlights
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(". ")
    };

    let excerpt = fragments
        .into_iter()
        .take(120)
        .collect::<Vec<_>>()
        .join("\n");
    Some(McpInstallGuideDocsContext {
        url: Some(url.to_string()),
        summary,
        highlights,
        text_excerpt: excerpt,
    })
}

// ── Service / env inference ──────────────────────────────────────

fn infer_service_tag(req: &McpInstallGuideRequest, spec: &McpInstallGuideEnvSpec) -> &'static str {
    let text = format!(
        "{} {} {} {} {}",
        req.id,
        req.display_name.clone().unwrap_or_default(),
        req.description.clone().unwrap_or_default(),
        req.docs_url.clone().unwrap_or_default(),
        spec.description.clone().unwrap_or_default()
    )
    .to_ascii_lowercase();
    if text.contains("google") || spec.key.to_ascii_lowercase().starts_with("google_") {
        "google"
    } else if text.contains("github") || spec.key.to_ascii_lowercase().contains("github") {
        "github"
    } else if text.contains("notion") || spec.key.to_ascii_lowercase().contains("notion") {
        "notion"
    } else if text.contains("aws") || spec.key.to_ascii_lowercase().contains("aws_") {
        "aws"
    } else {
        "generic"
    }
}

fn is_generic_where_to_get(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("open docs")
        || lower.contains("apri la documentazione")
        || lower.contains("apri i docs")
        || lower.contains("service dashboard")
        || lower.contains("dashboard del servizio")
        || lower.contains("open the server documentation")
}

// ── Fallback env help generation ─────────────────────────────────

fn fallback_env_help_for_spec(
    spec: &McpInstallGuideEnvSpec,
    req: &McpInstallGuideRequest,
    docs: Option<&McpInstallGuideDocsContext>,
    language: GuideLanguage,
) -> McpInstallGuideEnvHelp {
    let key = spec.key.trim();
    let k = key.to_ascii_lowercase();
    let service = infer_service_tag(req, spec);
    let is_secret = spec.secret.unwrap_or(false)
        || k.contains("token")
        || k.contains("secret")
        || k.contains("password")
        || k.contains("api_key")
        || k.contains("authorization");

    let mut out = McpInstallGuideEnvHelp {
        key: key.to_string(),
        why: spec
            .description
            .clone()
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| {
                if language.is_italian() {
                    "Richiesto dal server MCP per autenticazione o configurazione dell'accesso."
                        .to_string()
                } else {
                    "Required by the MCP server to authenticate or configure access.".to_string()
                }
            }),
        where_to_get: req
            .docs_url
            .as_ref()
            .map(|d| {
                if language.is_italian() {
                    format!(
                        "Apri {} e cerca \"{}\" nelle sezioni installazione, autenticazione o variabili ambiente.",
                        d, key
                    )
                } else {
                    format!(
                        "Open {} and search \"{}\" in Installation/Authentication/Environment Variables.",
                        d, key
                    )
                }
            })
            .unwrap_or_else(|| {
                if language.is_italian() {
                    format!(
                        "Apri la documentazione del server e cerca \"{}\" nelle sezioni autenticazione o environment.",
                        key
                    )
                } else {
                    format!(
                        "Open server docs and search \"{}\" in authentication/environment sections.",
                        key
                    )
                }
            }),
        format_hint: if k.contains("authorization") {
            format!("{key}=Bearer <token>")
        } else if is_secret {
            format!("{key}=<secret>")
        } else if k.contains("url") || k.contains("endpoint") {
            format!("{key}=https://...")
        } else {
            format!("{key}=<value>")
        },
        vault_hint: if language.is_italian() {
            format!(
                "Preferisci un riferimento Vault: {key}=vault://mcp.{}",
                k.replace('_', ".")
            )
        } else {
            format!(
                "Prefer vault reference: {key}=vault://mcp.{}",
                k.replace('_', ".")
            )
        },
        retrieval_steps: vec![
            if language.is_italian() {
                format!("Trova `{}` nella documentazione env/auth del server.", key)
            } else {
                format!("Find `{}` in the server env/auth documentation.", key)
            },
            if language.is_italian() {
                "Genera o copia il valore richiesto dalla dashboard o console del provider."
                    .to_string()
            } else {
                "Generate or copy the required value from the provider dashboard/console."
                    .to_string()
            },
            if language.is_italian() {
                "Salvalo nel Vault e usa un riferimento vault:// nelle env.".to_string()
            } else {
                "Save in Vault and use vault:// reference in env.".to_string()
            },
        ],
    };

    if service == "github" && (k.contains("token") || k.contains("pat")) {
        out.why = if language.is_italian() {
            "Token GitHub usato per accedere a repository, issue e pull request.".to_string()
        } else {
            "GitHub token used to access repositories, issues, and pull requests.".to_string()
        };
        out.where_to_get =
            "GitHub -> Settings -> Developer settings -> Personal access tokens.".to_string();
        out.retrieval_steps = vec![
            if language.is_italian() {
                "Apri la pagina GitHub Personal Access Tokens.".to_string()
            } else {
                "Open GitHub Personal Access Tokens page.".to_string()
            },
            if language.is_italian() {
                "Crea un token con gli scope richiesti da questa integrazione MCP.".to_string()
            } else {
                "Create token with scopes required by this MCP integration.".to_string()
            },
            if language.is_italian() {
                "Copia il token una sola volta e salvalo nel Vault.".to_string()
            } else {
                "Copy token once and store it in Vault.".to_string()
            },
        ];
    } else if service == "notion" && k.contains("token") {
        out.why = if language.is_italian() {
            "Token di integrazione Notion per accedere a workspace e database.".to_string()
        } else {
            "Notion integration token for workspace/database access.".to_string()
        };
        out.where_to_get =
            "Notion -> Settings & members -> Integrations -> Develop your own integrations."
                .to_string();
    } else if service == "google"
        && (k.contains("google_client_id") || k.contains("google_client_secret"))
    {
        out.why = if language.is_italian() {
            "Credenziali OAuth client usate per le API Google.".to_string()
        } else {
            "OAuth client credentials used for Google APIs.".to_string()
        };
        out.where_to_get =
            "Google Cloud Console -> APIs & Services -> Credentials -> OAuth client ID."
                .to_string();
        out.retrieval_steps = vec![
            if language.is_italian() {
                "Crea o seleziona un progetto in Google Cloud.".to_string()
            } else {
                "Create/select project in Google Cloud.".to_string()
            },
            if language.is_italian() {
                "Configura la schermata consenso OAuth e gli scope richiesti.".to_string()
            } else {
                "Configure OAuth consent screen and required scopes.".to_string()
            },
            if language.is_italian() {
                "Crea un OAuth Client ID e copia client_id e client_secret.".to_string()
            } else {
                "Create OAuth Client ID and copy client_id/client_secret.".to_string()
            },
        ];
    } else if service == "google" && k.contains("google_refresh_token") {
        out.why = if language.is_italian() {
            "Refresh token per ottenere nuovi access token Google senza rifare il login."
                .to_string()
        } else {
            "Refresh token to obtain new Google access tokens without re-login.".to_string()
        };
        out.where_to_get = if language.is_italian() {
            "Si genera durante il flusso OAuth con accesso offline abilitato.".to_string()
        } else {
            "Generate during OAuth consent flow with offline access enabled.".to_string()
        };
        out.retrieval_steps = vec![
            if language.is_italian() {
                "Esegui il flusso OAuth authorization code con offline access.".to_string()
            } else {
                "Run OAuth authorization code flow with offline access.".to_string()
            },
            if language.is_italian() {
                "Scambia il codice autorizzativo con i token e copia refresh_token.".to_string()
            } else {
                "Exchange auth code for tokens and copy refresh_token.".to_string()
            },
            if language.is_italian() {
                "Usa il refresh token nelle env per evitare aggiornamenti manuali frequenti."
                    .to_string()
            } else {
                "Use refresh token in env to avoid frequent manual updates.".to_string()
            },
        ];
    } else if service == "google" && k.contains("google_access_token") {
        out.why = if language.is_italian() {
            "Access token OAuth Google a breve durata usato per autorizzare le API.".to_string()
        } else {
            "Short-lived Google OAuth access token for API authorization.".to_string()
        };
        out.where_to_get = if language.is_italian() {
            "Si ottiene dallo scambio token OAuth (OAuth Playground o il tuo flusso OAuth)."
                .to_string()
        } else {
            "Generated by OAuth token exchange (OAuth Playground or your OAuth flow).".to_string()
        };
        out.retrieval_steps = vec![
            if language.is_italian() {
                "Autorizza l'app ed esegui lo scambio codice presso l'endpoint token OAuth Google."
                    .to_string()
            } else {
                "Authorize and exchange code at Google OAuth token endpoint.".to_string()
            },
            if language.is_italian() {
                "Copia `access_token` e annota la scadenza.".to_string()
            } else {
                "Copy `access_token` and note expiration time.".to_string()
            },
            if language.is_italian() {
                "Se supportato, preferisci il refresh token invece di aggiornare manualmente l'access token."
                    .to_string()
            } else {
                "If supported, prefer refresh token flow instead of manual access token updates."
                    .to_string()
            },
        ];
    } else if service == "aws"
        && (k.contains("aws_access_key_id") || k.contains("aws_secret_access_key"))
    {
        out.why = if language.is_italian() {
            "Credenziali AWS IAM usate dal server MCP.".to_string()
        } else {
            "AWS IAM credentials used by the MCP server.".to_string()
        };
        out.where_to_get =
            "AWS Console -> IAM -> Users -> Security credentials -> Create access key.".to_string();
    } else if k.contains("refresh_token") {
        out.why = if language.is_italian() {
            "Refresh token usato per ottenere automaticamente nuovi access token.".to_string()
        } else {
            "Refresh token used to obtain new access tokens automatically.".to_string()
        };
        out.where_to_get = if language.is_italian() {
            "Generalo in un flusso OAuth Authorization Code con offline access o scope equivalente."
                .to_string()
        } else {
            "Generate in OAuth Authorization Code flow with offline access/scope enabled."
                .to_string()
        };
    } else if k.contains("access_token") {
        out.why = if language.is_italian() {
            "Access token usato per autorizzare le API, di solito con durata breve.".to_string()
        } else {
            "Access token used for API authorization, usually short-lived.".to_string()
        };
        out.where_to_get = if language.is_italian() {
            "Generalo tramite endpoint OAuth/token del provider o dashboard API token.".to_string()
        } else {
            "Generate through provider OAuth/token endpoint or API token dashboard.".to_string()
        };
    } else if k.contains("token") || k.contains("api_key") || k.contains("secret") {
        out.why = if language.is_italian() {
            "Credenziale di autenticazione richiesta dal servizio o API di destinazione."
                .to_string()
        } else {
            "Authentication credential required by the target API/service.".to_string()
        };
        out.where_to_get = req
            .docs_url
            .as_deref()
            .map(|d| {
                if language.is_italian() {
                    format!(
                        "Apri la documentazione e segui la sezione autenticazione: {}",
                        d
                    )
                } else {
                    format!("Open docs and follow authentication section: {}", d)
                }
            })
            .unwrap_or_else(|| {
                if language.is_italian() {
                    "Apri la dashboard del servizio e crea un token o API key.".to_string()
                } else {
                    "Open the service dashboard and create an API token/key.".to_string()
                }
            });
    } else if k.contains("authorization") {
        out.why = if language.is_italian() {
            "Valore dell'header Authorization richiesto dall'endpoint MCP remoto.".to_string()
        } else {
            "Authorization header value expected by the remote MCP endpoint.".to_string()
        };
        out.where_to_get = req
            .docs_url
            .as_deref()
            .map(|d| {
                if language.is_italian() {
                    format!(
                        "Controlla la documentazione per il formato dell'header (es. Bearer token): {}",
                        d
                    )
                } else {
                    format!("Check docs for header format (e.g. Bearer token): {}", d)
                }
            })
            .unwrap_or_else(|| {
                if language.is_italian() {
                    "Controlla la documentazione dell'endpoint per il formato richiesto dell'header Authorization."
                        .to_string()
                } else {
                    "Check endpoint docs for required Authorization header format.".to_string()
                }
            });
        out.format_hint = format!("{key}=Bearer <token>");
    }

    if let Some(docs) = docs {
        let clues = find_relevant_doc_fragments(docs, Some(spec), 3);
        if !clues.is_empty() {
            out.where_to_get = if let Some(url) = docs.url.as_deref() {
                if language.is_italian() {
                    format!("Apri {} e cerca questo passaggio: {}", url, clues[0])
                } else {
                    format!("Open {} and look for: {}", url, clues[0])
                }
            } else if language.is_italian() {
                format!("Cerca nella documentazione questo passaggio: {}", clues[0])
            } else {
                format!("Look in documentation for: {}", clues[0])
            };
            out.retrieval_steps = clues
                .iter()
                .map(|clue| {
                    if language.is_italian() {
                        format!("Nella documentazione trova questo indizio: {}", clue)
                    } else {
                        format!("In the docs, locate this clue: {}", clue)
                    }
                })
                .collect();
            if out.retrieval_steps.len() < 3 {
                out.retrieval_steps.push(if language.is_italian() {
                    "Copia il formato esatto del valore dalla documentazione, poi salva i segreti nel Vault."
                        .to_string()
                } else {
                    "Copy the exact value format from docs, then store secrets in Vault."
                        .to_string()
                });
            }
        }
    }

    out
}

// ── Fallback guide builder ───────────────────────────────────────

fn build_fallback_install_guide(
    req: &McpInstallGuideRequest,
    docs: Option<&McpInstallGuideDocsContext>,
    error: Option<String>,
    language: GuideLanguage,
) -> McpInstallGuideResponse {
    let env_specs = req.env.clone().unwrap_or_default();
    let env_help = env_specs
        .iter()
        .map(|spec| fallback_env_help_for_spec(spec, req, docs, language))
        .collect::<Vec<_>>();

    let display_name = req
        .display_name
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(&req.id);
    let transport = req.transport.clone().unwrap_or_else(|| "stdio".to_string());
    let command = req.command.clone().unwrap_or_default();
    let args = req.args.clone().unwrap_or_default().join(" ");

    let mut steps = vec![
        if let Some(docs) = req.docs_url.clone().filter(|d| !d.trim().is_empty()) {
            if language.is_italian() {
                format!("Apri la documentazione di {}: {}", display_name, docs)
            } else {
                format!("Open docs for {}: {}", display_name, docs)
            }
        } else if language.is_italian() {
            format!("Rivedi questo server: {} ({})", display_name, req.id)
        } else {
            format!("Review this server: {} ({})", display_name, req.id)
        },
        if language.is_italian() {
            "Raccogli le credenziali richieste con la guida qui sotto e salva i segreti nel Vault (vault://...)".to_string()
        } else {
            "Collect required credentials with the guidance below and store secrets in Vault (vault://...)".to_string()
        },
        if language.is_italian() {
            "Salva la configurazione del server ed esegui Test per verificare la connessione."
                .to_string()
        } else {
            "Save the server configuration and run Test to validate connectivity.".to_string()
        },
    ];
    if transport == "http" {
        steps.insert(
            1,
            if language.is_italian() {
                "Conferma URL endpoint e valori richiesti per Authorization/header.".to_string()
            } else {
                "Confirm endpoint URL and required Authorization/header values".to_string()
            },
        );
    } else if !command.trim().is_empty() {
        steps.insert(
            1,
            if language.is_italian() {
                format!("Comando runtime: {} {}", command, args)
            } else {
                format!("Runtime command: {} {}", command, args)
            },
        );
    }

    McpInstallGuideResponse {
        ok: true,
        source: "fallback".to_string(),
        summary: if language.is_italian() {
            format!(
                "Configurazione guidata per {}. Leggi i passaggi, compila le variabili env, salva ed esegui il test di connessione.",
                display_name
            )
        } else {
            format!(
                "Guided setup for {}. Read the steps, fill env variables, save, and run connection test.",
                display_name
            )
        },
        steps,
        env_help,
        notes: vec![
            if language.is_italian() {
                "Usa riferimenti Vault per i segreti: KEY=vault://your.key".to_string()
            } else {
                "Use vault references for secrets: KEY=vault://your.key".to_string()
            },
            if language.is_italian() {
                "Se il test fallisce, verifica scope API, permessi e impostazioni di consenso."
                    .to_string()
            } else {
                "If test fails, verify API scopes/permissions and consent settings.".to_string()
            },
        ],
        documentation: docs.and_then(docs_context_to_view),
        error,
    }
}

// ── LLM guide generation ─────────────────────────────────────────

async fn try_generate_install_guide_with_llm(
    config: &crate::config::Config,
    req: &McpInstallGuideRequest,
    docs: Option<&McpInstallGuideDocsContext>,
    language: GuideLanguage,
) -> anyhow::Result<McpInstallGuideResponse> {
    let req_json = serde_json::to_string_pretty(req)?;
    let docs_block = docs
        .map(|d| {
            format!(
                "Documentation summary:\n{}\n\nDocumentation highlights:\n{}\n\nDocumentation excerpt:\n{}",
                d.summary,
                d.highlights.join("\n"),
                d.text_excerpt
            )
        })
        .unwrap_or_else(|| "Documentation summary:\nUnavailable".to_string());
    let system_prompt = format!(
        "You are an MCP installation assistant. Return ONLY valid JSON with keys: summary (string), steps (string[]), env_help (array of {{key,why,where_to_get,format_hint,vault_hint,retrieval_steps}}), notes (string[]). Keep concise and practical. Write every response string in {}.",
        language.llm_label()
    );
    let user_prompt = format!(
        "Prepare setup guidance for this MCP server config.\nInput JSON:\n{}\n\n{}\n\nRules:\n- steps max 6\n- env_help must include all env keys from input\n- where_to_get must include exact dashboard/console/doc path\n- each env_help item must include retrieval_steps with 2-4 concrete actions\n- use the provided documentation when available\n- explain where a dummy user can actually retrieve the missing credentials\n- no markdown",
        req_json,
        docs_block
    );

    let response = crate::provider::llm_one_shot(
        config,
        crate::provider::OneShotRequest {
            system_prompt,
            user_message: user_prompt,
            max_tokens: 700,
            temperature: 0.2,
            timeout_secs: 20,
            ..Default::default()
        },
    )
    .await?;

    let json_block = extract_json_object_block(&response.content)
        .ok_or_else(|| anyhow::anyhow!("could not extract JSON from llm response"))?;
    let parsed: McpInstallGuideLlmParsed =
        serde_json::from_str(json_block).map_err(|e| anyhow::anyhow!("invalid llm JSON: {e}"))?;

    let mut by_key = parsed
        .env_help
        .iter()
        .map(|h| (h.key.to_ascii_lowercase(), h.clone()))
        .collect::<HashMap<_, _>>();
    let env_help = req
        .env
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|spec| {
            let llm = by_key.remove(&spec.key.to_ascii_lowercase());
            let fallback = fallback_env_help_for_spec(&spec, req, docs, language);
            match llm {
                Some(mut help) => {
                    if help.key.trim().is_empty() {
                        help.key = spec.key.clone();
                    }
                    if help.why.trim().is_empty() {
                        help.why = fallback.why.clone();
                    }
                    if help.where_to_get.trim().is_empty()
                        || is_generic_where_to_get(&help.where_to_get)
                    {
                        help.where_to_get = fallback.where_to_get.clone();
                    }
                    if help.format_hint.trim().is_empty() {
                        help.format_hint = fallback.format_hint.clone();
                    }
                    if help.vault_hint.trim().is_empty() {
                        help.vault_hint = fallback.vault_hint.clone();
                    }
                    if help.retrieval_steps.is_empty() {
                        help.retrieval_steps = fallback.retrieval_steps.clone();
                    }
                    help
                }
                None => fallback,
            }
        })
        .collect::<Vec<_>>();

    Ok(McpInstallGuideResponse {
        ok: true,
        source: if docs.is_some() {
            "llm+docs".to_string()
        } else {
            "llm".to_string()
        },
        summary: parsed.summary,
        steps: parsed.steps,
        env_help,
        notes: parsed.notes,
        documentation: docs.and_then(docs_context_to_view),
        error: None,
    })
}

// ── Main handler ─────────────────────────────────────────────────

pub(super) async fn mcp_install_guide(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpInstallGuideRequest>,
) -> Json<McpInstallGuideResponse> {
    let config = state.config.read().await.clone();
    let language = GuideLanguage::from_request(req.language.as_deref());
    let docs = fetch_install_docs_context(&req).await;
    match try_generate_install_guide_with_llm(&config, &req, docs.as_ref(), language).await {
        Ok(out) => Json(out),
        Err(e) => {
            let mut out =
                build_fallback_install_guide(&req, docs.as_ref(), Some(e.to_string()), language);
            if docs.is_some() {
                out.source = "docs".to_string();
            }
            Json(out)
        }
    }
}
