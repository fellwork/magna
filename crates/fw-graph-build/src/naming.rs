/// Convert a postgres snake_case name to a GraphQL PascalCase type name.
/// Singularizes the last word, then applies PascalCase.
/// e.g. "user_profiles" → "UserProfile"
pub fn to_type_name(pg_name: &str) -> String {
    let parts: Vec<&str> = pg_name.split('_').collect();
    let mut words: Vec<String> = parts.iter().map(|s| capitalize(s)).collect();
    // Singularize the last word
    if let Some(last) = words.last_mut() {
        *last = singularize(last);
    }
    words.join("")
}

/// Convert a postgres snake_case column name to a GraphQL camelCase field name.
/// e.g. "first_name" → "firstName"
pub fn to_field_name(pg_name: &str) -> String {
    to_camel_case(pg_name)
}

/// Pluralize a PascalCase or plain name.
/// e.g. "User" → "Users", "Category" → "Categories"
pub fn pluralize(name: &str) -> String {
    if name.ends_with('y') && name.len() > 1 {
        let vowels = ['a', 'e', 'i', 'o', 'u'];
        let second_last = name.chars().rev().nth(1).unwrap_or('a');
        if !vowels.contains(&second_last) {
            // consonant + y → ies
            return format!("{}ies", &name[..name.len() - 1]);
        }
    }
    format!("{}s", name)
}

/// "User" → "UsersConnection"
pub fn connection_type_name(type_name: &str) -> String {
    format!("{}Connection", pluralize(type_name))
}

/// "User" → "UsersEdge"
pub fn edge_type_name(type_name: &str) -> String {
    format!("{}Edge", pluralize(type_name))
}

/// "User" → "UserFilter"
pub fn filter_type_name(type_name: &str) -> String {
    format!("{}Filter", type_name)
}

/// "User" → "UsersOrderBy"
pub fn order_by_type_name(type_name: &str) -> String {
    format!("{}OrderBy", pluralize(type_name))
}

/// "User" → "UserCondition"
pub fn condition_type_name(type_name: &str) -> String {
    format!("{}Condition", type_name)
}

/// "User" → "CreateUserInput"
pub fn create_input_type_name(type_name: &str) -> String {
    format!("Create{}Input", type_name)
}

/// "User" → "UserPatch"
pub fn patch_type_name(type_name: &str) -> String {
    format!("{}Patch", type_name)
}

/// "User" → "UpdateUserInput"
pub fn update_input_type_name(type_name: &str) -> String {
    format!("Update{}Input", type_name)
}

/// "User" → "DeleteUserInput"
pub fn delete_input_type_name(type_name: &str) -> String {
    format!("Delete{}Input", type_name)
}

/// "User" → "CreateUserPayload"
pub fn create_payload_type_name(type_name: &str) -> String {
    format!("Create{}Payload", type_name)
}

/// "User" → "UpdateUserPayload"
pub fn update_payload_type_name(type_name: &str) -> String {
    format!("Update{}Payload", type_name)
}

/// "User" → "DeleteUserPayload"
pub fn delete_payload_type_name(type_name: &str) -> String {
    format!("Delete{}Payload", type_name)
}

/// ("user_id", "users") → "userByUserId"
pub fn belongs_to_field_name(fk_col: &str, target_table: &str) -> String {
    let type_name = to_type_name(target_table);
    let col_field = to_pascal_case(fk_col);
    let base = format!("{}By{}", to_camel_case_lower_first(&type_name), col_field);
    base
}

/// ("author_id", "posts") → "postsByAuthorId"
pub fn has_many_field_name(fk_col: &str, source_table: &str) -> String {
    let type_name = to_type_name(source_table);
    let plural = pluralize(&type_name);
    let col_field = to_pascal_case(fk_col);
    format!("{}By{}", to_camel_case_lower_first(&plural), col_field)
}

/// "User" → "allUsers"
pub fn all_query_field_name(type_name: &str) -> String {
    format!("all{}", pluralize(type_name))
}

/// "User" → "userById"
pub fn by_pk_query_field_name(type_name: &str) -> String {
    format!("{}ById", to_camel_case_lower_first(type_name))
}

/// "User" → "createUser"
pub fn create_mutation_field_name(type_name: &str) -> String {
    format!("create{}", type_name)
}

/// "User" → "updateUser"
pub fn update_mutation_field_name(type_name: &str) -> String {
    format!("update{}", type_name)
}

/// "User" → "deleteUser"
pub fn delete_mutation_field_name(type_name: &str) -> String {
    format!("delete{}", type_name)
}

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Capitalize the first letter of a word.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// snake_case → PascalCase (all words capitalized).
pub fn to_pascal_case(s: &str) -> String {
    s.split('_').map(capitalize).collect()
}

/// snake_case → camelCase (first word lower, rest capitalized).
pub fn to_camel_case(s: &str) -> String {
    let mut parts = s.split('_');
    match parts.next() {
        None => String::new(),
        Some(first) => {
            let rest: String = parts.map(capitalize).collect();
            format!("{}{}", first.to_lowercase(), rest)
        }
    }
}

/// PascalCase → camelCase (lower-case first character).
pub fn to_camel_case_lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().to_string() + chars.as_str(),
    }
}

/// Naive PostGraphile-compatible singularize.
/// - "ies" → "y"
/// - "ses"/"xes"/"shes"/"ches" → remove "es"
/// - trailing "s" (not "ss") → remove "s"
/// - else → unchanged
pub fn singularize(word: &str) -> String {
    if word.ends_with("ies") {
        return format!("{}y", &word[..word.len() - 3]);
    }
    for suffix in &["ses", "xes", "shes", "ches"] {
        if word.ends_with(suffix) {
            return word[..word.len() - 2].to_string();
        }
    }
    if word.ends_with('s') && !word.ends_with("ss") {
        return word[..word.len() - 1].to_string();
    }
    word.to_string()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_type_name() {
        assert_eq!(to_type_name("user_profiles"), "UserProfile");
        assert_eq!(to_type_name("users"), "User");
        assert_eq!(to_type_name("categories"), "Category");
        assert_eq!(to_type_name("posts"), "Post");
    }

    #[test]
    fn test_to_field_name() {
        assert_eq!(to_field_name("first_name"), "firstName");
        assert_eq!(to_field_name("id"), "id");
        assert_eq!(to_field_name("user_profile_id"), "userProfileId");
    }

    #[test]
    fn test_pluralize() {
        assert_eq!(pluralize("User"), "Users");
        assert_eq!(pluralize("Category"), "Categories");
        assert_eq!(pluralize("Post"), "Posts");
        // vowel + y should just add s
        assert_eq!(pluralize("Day"), "Days");
    }

    #[test]
    fn test_connection_type_name() {
        assert_eq!(connection_type_name("User"), "UsersConnection");
    }

    #[test]
    fn test_edge_type_name() {
        assert_eq!(edge_type_name("User"), "UsersEdge");
    }

    #[test]
    fn test_filter_type_name() {
        assert_eq!(filter_type_name("User"), "UserFilter");
    }

    #[test]
    fn test_order_by_type_name() {
        assert_eq!(order_by_type_name("User"), "UsersOrderBy");
    }

    #[test]
    fn test_condition_type_name() {
        assert_eq!(condition_type_name("User"), "UserCondition");
    }

    #[test]
    fn test_create_input_type_name() {
        assert_eq!(create_input_type_name("User"), "CreateUserInput");
    }

    #[test]
    fn test_patch_type_name() {
        assert_eq!(patch_type_name("User"), "UserPatch");
    }

    #[test]
    fn test_update_input_type_name() {
        assert_eq!(update_input_type_name("User"), "UpdateUserInput");
    }

    #[test]
    fn test_delete_input_type_name() {
        assert_eq!(delete_input_type_name("User"), "DeleteUserInput");
    }

    #[test]
    fn test_create_payload_type_name() {
        assert_eq!(create_payload_type_name("User"), "CreateUserPayload");
    }

    #[test]
    fn test_update_payload_type_name() {
        assert_eq!(update_payload_type_name("User"), "UpdateUserPayload");
    }

    #[test]
    fn test_delete_payload_type_name() {
        assert_eq!(delete_payload_type_name("User"), "DeleteUserPayload");
    }

    #[test]
    fn test_belongs_to_field_name() {
        assert_eq!(belongs_to_field_name("user_id", "users"), "userByUserId");
    }

    #[test]
    fn test_has_many_field_name() {
        assert_eq!(has_many_field_name("author_id", "posts"), "postsByAuthorId");
    }

    #[test]
    fn test_all_query_field_name() {
        assert_eq!(all_query_field_name("User"), "allUsers");
    }

    #[test]
    fn test_by_pk_query_field_name() {
        assert_eq!(by_pk_query_field_name("User"), "userById");
    }

    #[test]
    fn test_create_mutation_field_name() {
        assert_eq!(create_mutation_field_name("User"), "createUser");
    }

    #[test]
    fn test_update_mutation_field_name() {
        assert_eq!(update_mutation_field_name("User"), "updateUser");
    }

    #[test]
    fn test_delete_mutation_field_name() {
        assert_eq!(delete_mutation_field_name("User"), "deleteUser");
    }

    #[test]
    fn test_singularize() {
        assert_eq!(singularize("categories"), "category");
        assert_eq!(singularize("buses"), "bus");
        assert_eq!(singularize("foxes"), "fox");
        assert_eq!(singularize("dishes"), "dish");
        assert_eq!(singularize("churches"), "church");
        assert_eq!(singularize("users"), "user");
        assert_eq!(singularize("class"), "class"); // ss unchanged
        assert_eq!(singularize("fish"), "fish");   // no trailing s
    }
}
