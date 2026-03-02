//! Page snapshot generation for LLM consumption.
//!
//! Uses CDP `Accessibility.getFullAXTree` for accurate accessibility tree extraction,
//! similar to Playwright's approach in OpenClaw.
//!
//! Format example:
//! ```text
//! - button "Search" [ref=e1]
//! - textbox "Enter query" [ref=e2]
//! - link "About us" [ref=e3]
//!   - heading "Welcome" [ref=e4]
//! ```
//!
//! Supports role-based element resolution (OpenClaw compatible):
//! - ref=e1 → getByRole("button", {name: "Search"})

use anyhow::{Context, Result};
use chromiumoxide::cdp::browser_protocol::accessibility::{AxNode, AxValue, GetFullAxTreeParams};
use chromiumoxide::cdp::browser_protocol::dom::{
    BackendNodeId, GetDocumentParams, PushNodesByBackendIdsToFrontendParams,
    SetAttributeValueParams,
};
use chromiumoxide::page::Page;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Interactive roles that should always have refs
const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "checkbox",
    "radio",
    "combobox",
    "listbox",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "searchbox",
    "slider",
    "spinbutton",
    "switch",
    "tab",
    "treeitem",
];

/// Content roles that should have refs if they have names
const CONTENT_ROLES: &[&str] = &[
    "heading",
    "cell",
    "gridcell",
    "columnheader",
    "rowheader",
    "listitem",
    "article",
    "region",
    "main",
    "navigation",
    "img",
];

/// Structural roles (skipped in compact mode)
const STRUCTURAL_ROLES: &[&str] = &[
    "generic",
    "group",
    "list",
    "table",
    "row",
    "rowgroup",
    "grid",
    "treegrid",
    "menu",
    "menubar",
    "toolbar",
    "tablist",
    "tree",
    "directory",
    "document",
    "application",
    "presentation",
    "none",
];

/// Options for snapshot generation
#[derive(Debug, Clone, Default)]
pub struct SnapshotOptions {
    /// Only include interactive elements (buttons, links, inputs)
    pub interactive_only: bool,
    /// Remove unnamed structural elements and empty branches
    pub compact: bool,
    /// Maximum depth to include (None = unlimited)
    pub max_depth: Option<usize>,
    /// Maximum number of elements to include
    pub limit: Option<usize>,
}

/// Role-based reference for element resolution (OpenClaw compatible)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoleRef {
    /// ARIA role (button, textbox, link, etc.)
    pub role: String,
    /// Human-readable name/label
    pub name: Option<String>,
    /// Index for duplicate role+name combinations
    pub nth: Option<usize>,
}

/// Reference to an element on the page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementRef {
    /// Unique reference ID (e.g., "e1", "e2")
    pub ref_id: String,
    /// ARIA role (button, textbox, link, heading, etc.)
    pub role: String,
    /// Human-readable name/label
    pub name: String,
    /// CSS selector for finding this element (fallback)
    pub selector: String,
    /// Depth in the tree (for indentation)
    #[serde(skip)]
    pub depth: usize,
    /// Role-based reference for getByRole resolution
    #[serde(skip)]
    pub role_ref: RoleRef,
}

/// Snapshot of a page's accessibility tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSnapshot {
    /// Current page URL
    pub url: String,
    /// Page title
    pub title: String,
    /// Page description/meta description if available
    pub description: Option<String>,
    /// Elements in accessibility tree format
    pub elements: Vec<ElementRef>,
    /// Role-based refs map for element resolution
    #[serde(skip)]
    pub role_refs: HashMap<String, RoleRef>,
    /// Formatted text snapshot for LLM
    #[serde(skip)]
    pub text_snapshot: String,
    /// Statistics about the snapshot
    pub stats: SnapshotStats,
}

/// Statistics about the snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotStats {
    /// Total number of elements
    pub total: usize,
    /// Number of interactive elements
    pub interactive: usize,
    /// Number of elements with refs
    pub with_refs: usize,
}

impl PageSnapshot {
    /// Create a snapshot from a page using CDP accessibility tree.
    pub async fn from_page(page: &Page) -> Result<Self> {
        Self::from_page_with_options(page, SnapshotOptions::default()).await
    }

    /// Create a snapshot with custom options.
    pub async fn from_page_with_options(page: &Page, options: SnapshotOptions) -> Result<Self> {
        // Get URL and title
        let url = page
            .url()
            .await
            .context("Failed to get page URL")?
            .map(|u| u.to_string())
            .unwrap_or_else(|| "about:blank".to_string());

        let title = page
            .get_title()
            .await
            .context("Failed to get page title")?
            .unwrap_or_else(|| "Untitled".to_string());

        // Get meta description via JavaScript
        let description = Self::get_meta_description(page).await?;

        // Extract accessibility tree via CDP
        let ax_nodes = Self::fetch_accessibility_tree(page).await?;

        // Process the tree into our format
        let (elements, role_refs, backend_map) =
            Self::process_accessibility_tree(&ax_nodes, &options);

        // Tag DOM elements with data-homun-ref attributes for reliable resolution
        if let Err(e) = Self::tag_elements_in_dom(page, &backend_map).await {
            tracing::warn!(error = ?e, "Failed to tag DOM elements (element resolution will use fallback)");
        }

        // Build text snapshot
        let text_snapshot = Self::format_accessibility_tree(&url, &title, &description, &elements);

        // Calculate stats
        let interactive = elements
            .iter()
            .filter(|e| INTERACTIVE_ROLES.contains(&e.role.as_str()))
            .count();
        let stats = SnapshotStats {
            total: elements.len(),
            interactive,
            with_refs: role_refs.len(),
        };

        Ok(Self {
            url,
            title,
            description,
            elements,
            role_refs,
            text_snapshot,
            stats,
        })
    }

    /// Fetch the full accessibility tree using CDP
    async fn fetch_accessibility_tree(page: &Page) -> Result<Vec<AxNode>> {
        let result = page
            .execute(GetFullAxTreeParams::default())
            .await
            .context("Failed to fetch accessibility tree via CDP")?;

        Ok(result.result.nodes)
    }

    /// Get meta description from the page
    async fn get_meta_description(page: &Page) -> Result<Option<String>> {
        let js = r#"
            (function() {
                const meta = document.querySelector('meta[name="description"]');
                return meta ? meta.getAttribute('content') : null;
            })();
        "#;

        let result = page
            .evaluate(js)
            .await
            .context("Failed to get meta description")?;

        Ok(result.into_value().ok().flatten())
    }

    /// Tag DOM elements with `data-homun-ref` attributes for reliable element resolution.
    ///
    /// Uses CDP to:
    /// 1. Clean up old tags
    /// 2. Convert BackendNodeIds (from AX tree) to frontend NodeIds
    /// 3. Set `data-homun-ref="eN"` on each element
    async fn tag_elements_in_dom(
        page: &Page,
        backend_map: &HashMap<String, BackendNodeId>,
    ) -> Result<()> {
        if backend_map.is_empty() {
            return Ok(());
        }

        // Step 0: Remove old data-homun-ref attributes
        let _ = page
            .evaluate(
                "document.querySelectorAll('[data-homun-ref]').forEach(el => el.removeAttribute('data-homun-ref'))",
            )
            .await;

        // Step 1: Initialize DOM agent (required before pushNodesByBackendIdsToFrontend)
        page.execute(GetDocumentParams::builder().depth(0).build())
            .await
            .context("Failed to initialize DOM agent via getDocument")?;

        // Step 2: Collect ref_ids and their backend IDs in order
        let ref_ids: Vec<&String> = backend_map.keys().collect();
        let backend_ids: Vec<BackendNodeId> =
            ref_ids.iter().map(|k| backend_map[*k]).collect();

        // Step 3: Batch convert BackendNodeId → NodeId (single CDP call)
        let push_result = page
            .execute(PushNodesByBackendIdsToFrontendParams::new(backend_ids))
            .await
            .context("Failed to push backend nodes to frontend")?;

        let node_ids = &push_result.result.node_ids;

        // Step 4: Set data-homun-ref attribute on each element
        let mut tagged = 0;
        for (i, node_id) in node_ids.iter().enumerate() {
            // NodeId of 0 means the node couldn't be resolved (e.g., detached)
            if *node_id.inner() == 0 {
                continue;
            }
            let ref_id = ref_ids[i];
            let params = SetAttributeValueParams::new(
                *node_id,
                "data-homun-ref",
                ref_id.as_str(),
            );
            if page.execute(params).await.is_ok() {
                tagged += 1;
            }
        }

        tracing::debug!(total = backend_map.len(), tagged, "Tagged DOM elements with data-homun-ref");
        Ok(())
    }

    /// Process the accessibility tree into our format.
    /// Returns (elements, role_refs, backend_node_map) where backend_node_map
    /// maps ref_id → BackendNodeId for DOM tagging.
    fn process_accessibility_tree(
        nodes: &[AxNode],
        options: &SnapshotOptions,
    ) -> (
        Vec<ElementRef>,
        HashMap<String, RoleRef>,
        HashMap<String, BackendNodeId>,
    ) {
        // Build node lookup
        let by_id: HashMap<String, &AxNode> = nodes
            .iter()
            .map(|n| (n.node_id.as_ref().to_string(), n))
            .collect();

        // Find root (node that is not referenced as child)
        let referenced: HashSet<String> = nodes
            .iter()
            .flat_map(|n| n.child_ids.iter().flatten())
            .map(|id| id.as_ref().to_string())
            .collect();

        let root = nodes
            .iter()
            .find(|n| !referenced.contains(n.node_id.as_ref()))
            .or_else(|| nodes.first());

        let root_id = match root {
            Some(r) => r.node_id.as_ref().to_string(),
            None => return (Vec::new(), HashMap::new(), HashMap::new()),
        };

        // Track role+name combinations for nth assignment
        let mut role_name_counts: HashMap<(String, Option<String>), usize> = HashMap::new();
        let mut role_refs: HashMap<String, RoleRef> = HashMap::new();
        let mut backend_map: HashMap<String, BackendNodeId> = HashMap::new();
        let mut elements: Vec<ElementRef> = Vec::new();
        let mut ref_counter = 0;

        // DFS traversal
        let mut stack: Vec<(String, usize)> = vec![(root_id, 0)];
        let limit = options.limit.unwrap_or(100);

        while let Some((node_id, depth)) = stack.pop() {
            if elements.len() >= limit {
                break;
            }

            // Check depth limit
            if let Some(max_depth) = options.max_depth {
                if depth > max_depth {
                    continue;
                }
            }

            let node = match by_id.get(&node_id) {
                Some(n) => *n,
                None => continue,
            };

            // Skip ignored nodes
            if node.ignored {
                // Still process children
                if let Some(child_ids) = &node.child_ids {
                    for child_id in child_ids.iter().rev() {
                        stack.push((child_id.as_ref().to_string(), depth));
                    }
                }
                continue;
            }

            // Extract role and name
            let role = Self::extract_ax_value_string(&node.role);
            let name = Self::extract_ax_value_string(&node.name);

            // Skip nodes without a role
            if role.is_empty() {
                if let Some(child_ids) = &node.child_ids {
                    for child_id in child_ids.iter().rev() {
                        stack.push((child_id.as_ref().to_string(), depth));
                    }
                }
                continue;
            }

            let role_lower = role.to_lowercase();
            let is_interactive = INTERACTIVE_ROLES.contains(&role_lower.as_str());
            let is_content = CONTENT_ROLES.contains(&role_lower.as_str());
            let is_structural = STRUCTURAL_ROLES.contains(&role_lower.as_str());

            // Filter based on options
            if options.interactive_only && !is_interactive {
                // Still process children for interactive-only mode
                if let Some(child_ids) = &node.child_ids {
                    for child_id in child_ids.iter().rev() {
                        stack.push((child_id.as_ref().to_string(), depth + 1));
                    }
                }
                continue;
            }

            if options.compact && is_structural && name.is_empty() {
                if let Some(child_ids) = &node.child_ids {
                    for child_id in child_ids.iter().rev() {
                        stack.push((child_id.as_ref().to_string(), depth));
                    }
                }
                continue;
            }

            // Should this element have a ref?
            let should_have_ref = is_interactive || (is_content && !name.is_empty());

            if should_have_ref {
                ref_counter += 1;
                let ref_id = format!("e{}", ref_counter);

                // Track role+name for nth assignment
                let key = (
                    role_lower.clone(),
                    if name.is_empty() {
                        None
                    } else {
                        Some(name.clone())
                    },
                );
                let count = role_name_counts.entry(key.clone()).or_insert(0);
                *count += 1;
                let nth = if *count > 1 { Some(*count - 1) } else { None };

                // Create role ref
                let role_ref = RoleRef {
                    role: role_lower.clone(),
                    name: if name.is_empty() {
                        None
                    } else {
                        Some(name.clone())
                    },
                    nth,
                };
                role_refs.insert(ref_id.clone(), role_ref.clone());

                // Store backend DOM node ID for element tagging
                if let Some(backend_id) = &node.backend_dom_node_id {
                    backend_map.insert(ref_id.clone(), *backend_id);
                }

                // Generate CSS selector as fallback
                let selector = Self::generate_selector(&role_lower, &name, nth);

                elements.push(ElementRef {
                    ref_id,
                    role: role_lower,
                    name,
                    selector,
                    depth,
                    role_ref,
                });
            }

            // Process children (reversed for correct order)
            if let Some(child_ids) = &node.child_ids {
                for child_id in child_ids.iter().rev() {
                    stack.push((child_id.as_ref().to_string(), depth + 1));
                }
            }
        }

        // Remove nth from non-duplicates (like OpenClaw does)
        Self::cleanup_nth_from_non_duplicates(&mut role_refs, &role_name_counts);

        (elements, role_refs, backend_map)
    }

    /// Extract string value from AxValue
    fn extract_ax_value_string(value: &Option<AxValue>) -> String {
        match value {
            Some(ax_value) => match &ax_value.value {
                Some(v) => match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    _ => String::new(),
                },
                None => String::new(),
            },
            None => String::new(),
        }
    }

    /// Generate a CSS selector for the element (fallback for role-based)
    fn generate_selector(role: &str, name: &str, nth: Option<usize>) -> String {
        // Try to build a role-based selector first
        let base_selector: String = match role {
            "button" => "button".to_string(),
            "link" => "a".to_string(),
            "textbox" => "input, textarea".to_string(),
            "checkbox" => "input[type='checkbox']".to_string(),
            "radio" => "input[type='radio']".to_string(),
            "combobox" | "listbox" => "select".to_string(),
            "heading" => "h1, h2, h3, h4, h5, h6".to_string(),
            "img" => "img".to_string(),
            "listitem" => "li".to_string(),
            _ => format!("[role='{}']", role),
        };

        // Add name-based selector if available
        if !name.is_empty() {
            let escaped = name.replace('"', "\\\"");
            let selector_with_name = match role {
                "button" => format!("button:has-text(\"{}\")", escaped),
                "link" => format!("a:has-text(\"{}\")", escaped),
                "textbox" => {
                    format!("input[placeholder*='{}'], input[aria-label*='{}'], textarea[placeholder*='{}']",
                        escaped, escaped, escaped)
                }
                "img" => format!("img[alt*='{}']", escaped),
                _ => format!("{}:has-text(\"{}\")", base_selector, escaped),
            };

            if let Some(n) = nth {
                return format!("{}:nth({})", selector_with_name, n);
            }
            return selector_with_name;
        }

        if let Some(n) = nth {
            format!("{}:nth({})", base_selector, n)
        } else {
            base_selector.to_string()
        }
    }

    /// Remove nth from elements that are not duplicates (like OpenClaw)
    fn cleanup_nth_from_non_duplicates(
        role_refs: &mut HashMap<String, RoleRef>,
        role_name_counts: &HashMap<(String, Option<String>), usize>,
    ) {
        // Find keys with duplicates
        let duplicates: HashSet<_> = role_name_counts
            .iter()
            .filter(|(_, &count)| count > 1)
            .map(|(k, _)| k.clone())
            .collect();

        // Remove nth from non-duplicates
        for (_ref_id, role_ref) in role_refs.iter_mut() {
            let key = (role_ref.role.clone(), role_ref.name.clone());
            if !duplicates.contains(&key) {
                role_ref.nth = None;
            }
        }
    }

    /// Format the accessibility tree for LLM consumption (OpenClaw style).
    fn format_accessibility_tree(
        url: &str,
        title: &str,
        description: &Option<String>,
        elements: &[ElementRef],
    ) -> String {
        let mut output = String::new();

        // Page metadata
        output.push_str("# Page Snapshot\n\n");
        output.push_str(&format!("**URL:** {}\n", url));
        output.push_str(&format!("**Title:** {}\n", title));

        if let Some(ref desc) = description {
            if !desc.is_empty() {
                output.push_str(&format!("**Description:** {}\n", desc));
            }
        }

        output.push_str("\n---\n\n");

        if elements.is_empty() {
            output.push_str("(no interactive elements found)\n");
            return output;
        }

        for el in elements {
            // Adjust indentation based on depth
            let indent = "  ".repeat(el.depth.min(5));

            // Format: - role "name" [ref=e1]
            if el.name.is_empty() {
                output.push_str(&format!("{}- {} [ref={}]\n", indent, el.role, el.ref_id));
            } else {
                // Escape quotes in name
                let escaped_name = el.name.replace('"', "'");
                let mut line = format!(
                    "{}- {} \"{}\" [ref={}",
                    indent, el.role, escaped_name, el.ref_id
                );

                // Add nth for duplicates
                if let Some(nth) = el.role_ref.nth {
                    line.push_str(&format!("] [nth={}]", nth));
                } else {
                    line.push(']');
                }

                output.push_str(&line);
                output.push('\n');
            }
        }

        output.push_str("\n---\n");
        output.push_str("Use [ref=eX] to interact with elements. Example: click ref=e1\n");

        output
    }

    /// Format for LLM (returns the pre-built text snapshot).
    pub fn to_llm_format(&self) -> String {
        self.text_snapshot.clone()
    }

    /// Get a role ref by ID
    pub fn get_role_ref(&self, ref_id: &str) -> Option<&RoleRef> {
        self.role_refs.get(ref_id)
    }
}

impl std::fmt::Display for ElementRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.name.is_empty() {
            write!(f, "- {} [ref={}]", self.role, self.ref_id)
        } else {
            write!(f, "- {} \"{}\" [ref={}]", self.role, self.name, self.ref_id)
        }
    }
}

impl std::fmt::Display for PageSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text_snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_ref_format() {
        let el = ElementRef {
            ref_id: "e1".to_string(),
            role: "button".to_string(),
            name: "Submit".to_string(),
            selector: "#submit".to_string(),
            depth: 0,
            role_ref: RoleRef {
                role: "button".to_string(),
                name: Some("Submit".to_string()),
                nth: None,
            },
        };
        assert_eq!(format!("{}", el), "- button \"Submit\" [ref=e1]");
    }

    #[test]
    fn test_element_ref_no_name() {
        let el = ElementRef {
            ref_id: "e2".to_string(),
            role: "img".to_string(),
            name: String::new(),
            selector: "img.hero".to_string(),
            depth: 0,
            role_ref: RoleRef {
                role: "img".to_string(),
                name: None,
                nth: None,
            },
        };
        assert_eq!(format!("{}", el), "- img [ref=e2]");
    }

    #[test]
    fn test_role_ref_serialization() {
        let role_ref = RoleRef {
            role: "button".to_string(),
            name: Some("Submit".to_string()),
            nth: Some(2),
        };
        let json = serde_json::to_string(&role_ref).unwrap();
        assert!(json.contains("\"role\":\"button\""));
        assert!(json.contains("\"nth\":2"));
    }

    #[test]
    fn test_selector_generation() {
        // Button with name
        let selector = PageSnapshot::generate_selector("button", "Click me", None);
        assert!(selector.contains("button"));
        assert!(selector.contains("Click me"));

        // Link with nth
        let selector = PageSnapshot::generate_selector("link", "Read more", Some(3));
        assert!(selector.contains(":nth(3)"));

        // Textbox without name
        let selector = PageSnapshot::generate_selector("textbox", "", None);
        assert!(selector.contains("input"));
    }

    #[test]
    fn test_interactive_roles() {
        assert!(INTERACTIVE_ROLES.contains(&"button"));
        assert!(INTERACTIVE_ROLES.contains(&"link"));
        assert!(INTERACTIVE_ROLES.contains(&"textbox"));
        assert!(!INTERACTIVE_ROLES.contains(&"heading"));
    }

    #[test]
    fn test_accessibility_tree_format() {
        let mut role_refs = HashMap::new();
        role_refs.insert(
            "e1".to_string(),
            RoleRef {
                role: "button".to_string(),
                name: Some("Login".to_string()),
                nth: None,
            },
        );
        role_refs.insert(
            "e2".to_string(),
            RoleRef {
                role: "textbox".to_string(),
                name: Some("Email".to_string()),
                nth: None,
            },
        );
        role_refs.insert(
            "e3".to_string(),
            RoleRef {
                role: "link".to_string(),
                name: Some("Help".to_string()),
                nth: None,
            },
        );

        let elements = vec![
            ElementRef {
                ref_id: "e1".to_string(),
                role: "button".to_string(),
                name: "Login".to_string(),
                selector: "#login".to_string(),
                depth: 0,
                role_ref: role_refs["e1"].clone(),
            },
            ElementRef {
                ref_id: "e2".to_string(),
                role: "textbox".to_string(),
                name: "Email".to_string(),
                selector: "#email".to_string(),
                depth: 1,
                role_ref: role_refs["e2"].clone(),
            },
            ElementRef {
                ref_id: "e3".to_string(),
                role: "link".to_string(),
                name: "Help".to_string(),
                selector: "a.help".to_string(),
                depth: 0,
                role_ref: role_refs["e3"].clone(),
            },
        ];

        let description = Some("A sample site".to_string());
        let text_snapshot = PageSnapshot::format_accessibility_tree(
            "https://example.com",
            "Example Site",
            &description,
            &elements,
        );

        let snapshot = PageSnapshot {
            url: "https://example.com".to_string(),
            title: "Example Site".to_string(),
            description,
            elements,
            role_refs,
            text_snapshot,
            stats: SnapshotStats {
                total: 3,
                interactive: 3,
                with_refs: 3,
            },
        };

        let formatted = snapshot.to_llm_format();
        assert!(formatted.contains("**URL:** https://example.com"));
        assert!(formatted.contains("button \"Login\" [ref=e1]"));
        assert!(formatted.contains("textbox \"Email\" [ref=e2]"));
    }
}
