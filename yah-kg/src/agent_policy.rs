//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! Agent-policy rule schema — the authoring vocabulary that becomes
//! the "Roles / Do / Don't" block in the rendered CLAUDE.md prelude.
//!
//! Authors write `@yah:rule(agent-role: "name", "body")`,
//! `@yah:rule(agent-do: "...")`, or `@yah:rule(agent-dont: "...")`. The
//! annotation parser folds those rules onto the enclosing
//! `@yah:relay(...)` / `@yah:ticket(...)` work-item block when they
//! appear inside one (relay-scoped policy); otherwise they remain
//! standalone `RawAnnotation::Rule` annotations the daemon collects as
//! workspace-level defaults.
//!
//! Versioned: [`SCHEMA_VERSION`] bumps when the wire shape of
//! [`AgentPolicyRule`] changes. Forward-compatible parsing — unknown
//! `agent-*` kinds round-trip through [`AgentPolicyKind::Unknown`] so
//! a future kind authored before the consumer is upgraded surfaces as
//! a soft "skipped policy rule" rather than a hard parse error or a
//! silent drop.

use serde::{Deserialize, Serialize};

/// Wire-shape version. Bump when [`AgentPolicyRule`] grows or removes
/// fields; consumers reading an older snapshot's rules can branch on
/// this to translate. v1 = initial schema (role / do / dont).
pub const SCHEMA_VERSION: u32 = 1;

/// One agent-policy rule, parsed from a `@yah:rule(agent-...)` directive.
///
/// `body` is the human-readable text. For [`AgentPolicyKind::Role`],
/// `role_name` carries the first arg (e.g. `"Reviewer"`); `body` is the
/// second arg.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPolicyRule {
    pub kind: AgentPolicyKind,
    pub body: String,
    /// Only set for [`AgentPolicyKind::Role`]. The role's short name
    /// (e.g. `"Reviewer"`) so the renderer can title each role block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_name: Option<String>,
    /// The schema version this rule was authored under. Defaults to the
    /// current [`SCHEMA_VERSION`] when freshly parsed.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

/// Discriminator over the v1 agent-policy vocabulary. `Unknown` carries
/// the raw `agent-*` rule kind so the renderer can show an
/// "unrecognized agent-policy rule" line rather than silently dropping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "kebab-case")]
pub enum AgentPolicyKind {
    /// `@yah:rule(agent-role: "Reviewer", "You verify, you don't write.")`
    Role,
    /// `@yah:rule(agent-do: "Run cargo test before handoff.")`
    Do,
    /// `@yah:rule(agent-dont: "Refactor in flight tickets.")`
    Dont,
    /// `@yah:rule(agent-foo: ...)` — unrecognized agent-* kind.
    Unknown { raw: String },
}

/// True for rule kinds that belong to the agent-policy namespace. The
/// annotation parser consults this to decide whether to fold a
/// `@yah:rule(...)` directive onto the current work-item block (policy)
/// or emit it as a standalone `RawAnnotation::Rule` (graph-validator
/// vocabulary like `no-import-of`).
pub fn is_policy_rule_kind(rule_kind: &str) -> bool {
    let normalized = rule_kind.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "agent-role" | "agent-do" | "agent-dont"
    ) || normalized.starts_with("agent-")
}

/// Parse `(rule_kind, args)` into an [`AgentPolicyRule`]. Returns `None`
/// when the kind is outside the `agent-*` namespace — callers route to
/// the validator's vocabulary in that case. `Some(Err(_))` when the
/// kind is recognizably agent-policy but its args are malformed
/// (missing body, wrong arity).
pub fn parse_rule(rule_kind: &str, args: &[String]) -> Option<Result<AgentPolicyRule, String>> {
    if !is_policy_rule_kind(rule_kind) {
        return None;
    }
    let normalized = rule_kind.trim().to_ascii_lowercase();
    Some(match normalized.as_str() {
        "agent-role" => parse_role(rule_kind, args),
        "agent-do" => parse_text("agent-do", AgentPolicyKind::Do, args),
        "agent-dont" => parse_text("agent-dont", AgentPolicyKind::Dont, args),
        _ => Ok(AgentPolicyRule {
            kind: AgentPolicyKind::Unknown {
                raw: rule_kind.trim().to_string(),
            },
            body: args
                .iter()
                .map(|a| strip_quotes(a))
                .collect::<Vec<_>>()
                .join(", "),
            role_name: None,
            schema_version: SCHEMA_VERSION,
        }),
    })
}

fn parse_role(rule_kind: &str, args: &[String]) -> Result<AgentPolicyRule, String> {
    match args {
        [name, body, ..] => {
            let name = strip_quotes(name);
            let body = strip_quotes(body);
            if name.is_empty() {
                return Err(format!("{}: empty role name", rule_kind));
            }
            if body.is_empty() {
                return Err(format!("{}: empty role body", rule_kind));
            }
            Ok(AgentPolicyRule {
                kind: AgentPolicyKind::Role,
                body,
                role_name: Some(name),
                schema_version: SCHEMA_VERSION,
            })
        }
        [single] => Err(format!(
            "{}: expected `name, \"body\"` — got only one arg ({:?})",
            rule_kind, single
        )),
        _ => Err(format!(
            "{}: expected 2 args (name, body) — got {}",
            rule_kind,
            args.len()
        )),
    }
}

fn parse_text(
    rule_kind: &str,
    kind: AgentPolicyKind,
    args: &[String],
) -> Result<AgentPolicyRule, String> {
    let joined = args
        .iter()
        .map(|a| strip_quotes(a))
        .collect::<Vec<_>>()
        .join(", ");
    if joined.is_empty() {
        return Err(format!("{}: empty body", rule_kind));
    }
    Ok(AgentPolicyRule {
        kind,
        body: joined,
        role_name: None,
        schema_version: SCHEMA_VERSION,
    })
}

fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_agent_policy_kinds() {
        assert!(is_policy_rule_kind("agent-role"));
        assert!(is_policy_rule_kind("agent-do"));
        assert!(is_policy_rule_kind("agent-dont"));
        // Forward-compat: any agent-* prefix is reserved.
        assert!(is_policy_rule_kind("agent-must"));
        // Validator vocabulary stays out.
        assert!(!is_policy_rule_kind("no-import-of"));
        assert!(!is_policy_rule_kind("max-depth"));
    }

    #[test]
    fn parses_agent_role_with_name_and_body() {
        let rule = parse_rule(
            "agent-role",
            &[
                "\"Reviewer\"".to_string(),
                "\"You verify, you don't write.\"".to_string(),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(rule.kind, AgentPolicyKind::Role);
        assert_eq!(rule.role_name.as_deref(), Some("Reviewer"));
        assert_eq!(rule.body, "You verify, you don't write.");
        assert_eq!(rule.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn parses_agent_do_and_dont() {
        let r = parse_rule("agent-do", &["\"Run cargo test.\"".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(r.kind, AgentPolicyKind::Do);
        assert_eq!(r.body, "Run cargo test.");
        assert!(r.role_name.is_none());

        let r = parse_rule("agent-dont", &["\"Touch unrelated files.\"".to_string()])
            .unwrap()
            .unwrap();
        assert_eq!(r.kind, AgentPolicyKind::Dont);
        assert_eq!(r.body, "Touch unrelated files.");
    }

    #[test]
    fn agent_role_rejects_missing_body() {
        let err = parse_rule("agent-role", &["\"Reviewer\"".to_string()])
            .unwrap()
            .unwrap_err();
        assert!(err.contains("agent-role"), "{err}");
    }

    #[test]
    fn agent_do_rejects_empty_body() {
        let err = parse_rule("agent-do", &[]).unwrap().unwrap_err();
        assert!(err.contains("agent-do"), "{err}");
    }

    #[test]
    fn unknown_agent_prefix_round_trips_as_unknown() {
        let rule = parse_rule("agent-foo", &["\"x\"".to_string()])
            .unwrap()
            .unwrap();
        match rule.kind {
            AgentPolicyKind::Unknown { raw } => assert_eq!(raw, "agent-foo"),
            other => panic!("expected Unknown, got {:?}", other),
        }
        assert_eq!(rule.body, "x");
    }

    #[test]
    fn non_agent_kinds_are_not_policy_rules() {
        assert!(parse_rule("no-import-of", &["tag(view)".to_string()]).is_none());
    }
}
