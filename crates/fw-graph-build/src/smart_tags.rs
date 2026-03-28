/// Parsed smart tags from a pg_description comment.
///
/// Tags are lines starting with `@tag value`.
/// Non-tag lines are joined as the description.
#[derive(Debug, Clone, Default)]
pub struct SmartTags {
    pub name_override: Option<String>,
    pub omit: Vec<String>,
    pub behavior_add: Vec<String>,
    pub behavior_remove: Vec<String>,
    pub description: Option<String>,
}

/// Parse `@tag value` directives from a pg_description comment.
pub fn parse_smart_tags(comment: &str) -> SmartTags {
    let mut tags = SmartTags::default();
    let mut desc_lines: Vec<String> = Vec::new();

    for line in comment.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@name ") {
            tags.name_override = Some(rest.trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix("@omit ") {
            let values: Vec<String> = rest
                .split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect();
            tags.omit.extend(values);
        } else if let Some(rest) = trimmed.strip_prefix("@behavior ") {
            let rest = rest.trim();
            if let Some(val) = rest.strip_prefix('+') {
                tags.behavior_add.push(val.trim().to_string());
            } else if let Some(val) = rest.strip_prefix('-') {
                tags.behavior_remove.push(val.trim().to_string());
            }
        } else if !trimmed.is_empty() {
            desc_lines.push(trimmed.to_string());
        }
    }

    if !desc_lines.is_empty() {
        tags.description = Some(desc_lines.join("\n"));
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_comment() {
        let tags = parse_smart_tags("");
        assert!(tags.name_override.is_none());
        assert!(tags.omit.is_empty());
        assert!(tags.behavior_add.is_empty());
        assert!(tags.behavior_remove.is_empty());
        assert!(tags.description.is_none());
    }

    #[test]
    fn test_name_override() {
        let tags = parse_smart_tags("@name UserAccount");
        assert_eq!(tags.name_override.as_deref(), Some("UserAccount"));
        assert!(tags.description.is_none());
    }

    #[test]
    fn test_single_omit() {
        let tags = parse_smart_tags("@omit delete");
        assert_eq!(tags.omit, vec!["delete"]);
    }

    #[test]
    fn test_multiple_omit() {
        let tags = parse_smart_tags("@omit delete,update,create");
        assert_eq!(tags.omit, vec!["delete", "update", "create"]);
    }

    #[test]
    fn test_behavior_add() {
        let tags = parse_smart_tags("@behavior +connection");
        assert_eq!(tags.behavior_add, vec!["connection"]);
        assert!(tags.behavior_remove.is_empty());
    }

    #[test]
    fn test_behavior_remove() {
        let tags = parse_smart_tags("@behavior -insert");
        assert_eq!(tags.behavior_remove, vec!["insert"]);
        assert!(tags.behavior_add.is_empty());
    }

    #[test]
    fn test_description_preserved() {
        let tags = parse_smart_tags("A user account in the system.");
        assert_eq!(
            tags.description.as_deref(),
            Some("A user account in the system.")
        );
        assert!(tags.name_override.is_none());
    }

    #[test]
    fn test_mixed_tags_and_description() {
        let comment = "A user in the system.\n@name UserAccount\n@omit delete,update\nStores login info.";
        let tags = parse_smart_tags(comment);
        assert_eq!(tags.name_override.as_deref(), Some("UserAccount"));
        assert_eq!(tags.omit, vec!["delete", "update"]);
        assert_eq!(
            tags.description.as_deref(),
            Some("A user in the system.\nStores login info.")
        );
    }

    #[test]
    fn test_behavior_add_and_remove() {
        let comment = "@behavior +connection\n@behavior -insert\n@behavior -update";
        let tags = parse_smart_tags(comment);
        assert_eq!(tags.behavior_add, vec!["connection"]);
        assert_eq!(tags.behavior_remove, vec!["insert", "update"]);
    }
}
