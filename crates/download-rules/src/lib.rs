//! Smart download rules.
//!
//! Rules are pure data and evaluation is deterministic: enabled rules are
//! ordered by descending priority and then by their stable id. Every configured
//! condition must match; within a list, any entry may match.

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum RuleError {
    #[error("invalid regular expression: {0}")]
    Regex(#[from] regex::Error),
    #[error("invalid rule conditions: {0}")]
    InvalidConditions(String),
    #[error("rule name is required")]
    EmptyName,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleConditions {
    /// Extensions without the leading dot. Case-insensitive.
    pub extensions: Vec<String>,
    /// Domain suffixes. `example.com` also matches `cdn.example.com`.
    pub domains: Vec<String>,
    pub filename_regex: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub content_types: Vec<String>,
    pub url_regex: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleActions {
    pub category: Option<String>,
    pub destination_dir: Option<String>,
    pub queue_id: Option<String>,
    pub priority: Option<i32>,
    pub connections: Option<u32>,
    pub require_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    pub conditions: RuleConditions,
    pub actions: RuleActions,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct RuleContext<'a> {
    pub file_name: &'a str,
    pub url: &'a Url,
    pub size: Option<u64>,
    pub content_type: Option<&'a str>,
}

impl<'a> RuleContext<'a> {
    pub fn new(
        file_name: &'a str,
        url: &'a Url,
        size: Option<u64>,
        content_type: Option<&'a str>,
    ) -> Self {
        Self {
            file_name,
            url,
            size,
            content_type,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuleMatch<'a> {
    pub rule: &'a Rule,
    pub actions: &'a RuleActions,
}

/// Validate a rule before it is persisted.
pub fn validate_rule(rule: &Rule) -> Result<(), RuleError> {
    if rule.name.trim().is_empty() {
        return Err(RuleError::EmptyName);
    }
    if let Some(pattern) = rule.conditions.filename_regex.as_deref() {
        Regex::new(pattern)?;
    }
    if let Some(pattern) = rule.conditions.url_regex.as_deref() {
        Regex::new(pattern)?;
    }
    if let (Some(min), Some(max)) = (rule.conditions.min_size, rule.conditions.max_size) {
        if min > max {
            return Err(RuleError::InvalidConditions(
                "min_size must not exceed max_size".into(),
            ));
        }
    }
    Ok(())
}

fn extension_of(file_name: &str) -> String {
    std::path::Path::new(file_name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_ascii_lowercase()
}

fn domain_matches(host: &str, candidate: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let candidate = candidate
        .trim()
        .trim_start_matches("*.")
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    !candidate.is_empty() && (host == candidate || host.ends_with(&format!(".{candidate}")))
}

fn conditions_match(conditions: &RuleConditions, ctx: &RuleContext<'_>) -> bool {
    if !conditions.extensions.is_empty() {
        let extension = extension_of(ctx.file_name);
        let matched = conditions.extensions.iter().any(|candidate| {
            candidate
                .trim()
                .trim_start_matches('.')
                .eq_ignore_ascii_case(&extension)
        });
        if !matched {
            return false;
        }
    }

    if !conditions.domains.is_empty() {
        let host = ctx.url.host_str().unwrap_or_default();
        if !conditions
            .domains
            .iter()
            .any(|candidate| domain_matches(host, candidate))
        {
            return false;
        }
    }

    if let Some(min) = conditions.min_size {
        match ctx.size {
            Some(size) if size >= min => {}
            _ => return false,
        }
    }
    if let Some(max) = conditions.max_size {
        match ctx.size {
            Some(size) if size <= max => {}
            _ => return false,
        }
    }

    if !conditions.content_types.is_empty() {
        let content_type = ctx.content_type.unwrap_or_default().to_ascii_lowercase();
        if !conditions.content_types.iter().any(|candidate| {
            let candidate = candidate.trim().to_ascii_lowercase();
            content_type == candidate || content_type.starts_with(&format!("{candidate}/"))
        }) {
            return false;
        }
    }

    if let Some(pattern) = conditions.filename_regex.as_deref() {
        match Regex::new(pattern) {
            Ok(regex) if regex.is_match(ctx.file_name) => {}
            _ => return false,
        }
    }
    if let Some(pattern) = conditions.url_regex.as_deref() {
        match Regex::new(pattern) {
            Ok(regex) if regex.is_match(ctx.url.as_str()) => {}
            _ => return false,
        }
    }

    true
}

/// Evaluate all enabled rules. The first match wins, using a stable ordering.
pub fn evaluate<'a>(rules: &'a [Rule], ctx: &RuleContext<'_>) -> Option<RuleMatch<'a>> {
    let mut ordered: Vec<&Rule> = rules.iter().filter(|rule| rule.enabled).collect();
    ordered.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
    ordered
        .into_iter()
        .find(|rule| conditions_match(&rule.conditions, ctx))
        .map(|rule| RuleMatch {
            rule,
            actions: &rule.actions,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(extensions: &[&str], domains: &[&str], priority: i32) -> Rule {
        Rule {
            id: format!("rule-{priority}"),
            name: format!("Rule {priority}"),
            enabled: true,
            priority,
            conditions: RuleConditions {
                extensions: extensions.iter().map(|s| s.to_string()).collect(),
                domains: domains.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            actions: RuleActions {
                category: Some("videos".into()),
                ..Default::default()
            },
        }
    }

    #[test]
    fn matches_extension_and_domain() {
        let url = Url::parse("https://cdn.example.com/video.mp4").unwrap();
        let ctx = RuleContext::new("video.mp4", &url, Some(1000), Some("video/mp4"));
        let rules = vec![rule(&["mp4"], &["example.com"], 1)];
        assert!(evaluate(&rules, &ctx).is_some());
    }

    #[test]
    fn domain_suffix_is_exact_on_label_boundary() {
        let good = Url::parse("https://cdn.example.com/a.bin").unwrap();
        let bad = Url::parse("https://notexample.com/a.bin").unwrap();
        let rules = vec![rule(&[], &["example.com"], 1)];
        assert!(evaluate(&rules, &RuleContext::new("a.bin", &good, None, None)).is_some());
        assert!(evaluate(&rules, &RuleContext::new("a.bin", &bad, None, None)).is_none());
    }

    #[test]
    fn disabled_rules_are_ignored() {
        let url = Url::parse("https://example.com/video.mp4").unwrap();
        let mut r = rule(&["mp4"], &[], 1);
        r.enabled = false;
        assert!(evaluate(&[r], &RuleContext::new("video.mp4", &url, None, None)).is_none());
    }

    #[test]
    fn priority_breaks_ties_deterministically() {
        let url = Url::parse("https://example.com/video.mp4").unwrap();
        let rules = vec![rule(&["mp4"], &[], 1), rule(&["mp4"], &[], 5)];
        let matched = evaluate(&rules, &RuleContext::new("video.mp4", &url, None, None)).unwrap();
        assert_eq!(matched.rule.priority, 5);
    }

    #[test]
    fn size_bounds_require_known_size() {
        let url = Url::parse("https://example.com/a.bin").unwrap();
        let mut r = rule(&[], &[], 1);
        r.conditions.min_size = Some(100);
        assert!(evaluate(
            &[r.clone()],
            &RuleContext::new("a.bin", &url, Some(50), None)
        )
        .is_none());
        assert!(evaluate(&[r], &RuleContext::new("a.bin", &url, None, None)).is_none());
    }
}
