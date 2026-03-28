use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct GatherOutput {
    pub resources: Vec<ResolvedResource>,
    pub relations: Vec<ResolvedRelation>,
    pub behaviors: HashMap<String, BehaviorSet>,
    pub enums: Vec<ResolvedEnum>,
    pub smart_tags: HashMap<String, crate::smart_tags::SmartTags>,
    pub plugin_metadata: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ResolvedResource {
    pub name: String,
    pub schema: String,
    pub table: String,
    pub kind: ResourceKind,
    pub columns: Vec<ResolvedColumn>,
    pub primary_key: Vec<String>,
    pub unique_constraints: Vec<Vec<String>>,
    pub class_oid: u32,
}

#[derive(Debug, Clone)]
pub struct ResolvedColumn {
    pub pg_name: String,
    pub gql_name: String,
    pub type_oid: u32,
    pub gql_type: String,
    pub is_not_null: bool,
    pub has_default: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedRelation {
    pub name: String,
    pub source_resource: String,
    pub source_columns: Vec<String>,
    pub target_resource: String,
    pub target_columns: Vec<String>,
    pub is_unique: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedEnum {
    pub name: String,
    pub values: Vec<String>,
    pub pg_type_oid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Table,
    View,
    Function,
}

/// A bitflag set describing which GraphQL operations are enabled for a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BehaviorSet {
    flags: u16,
}

impl BehaviorSet {
    pub const CONNECTION: u16 = 0b0000_0001;
    pub const SELECT_ONE: u16 = 0b0000_0010;
    pub const INSERT: u16 = 0b0000_0100;
    pub const UPDATE: u16 = 0b0000_1000;
    pub const DELETE: u16 = 0b0001_0000;
    pub const FILTER: u16 = 0b0010_0000;
    pub const ORDER_BY: u16 = 0b0100_0000;

    const ALL_FLAGS: u16 = Self::CONNECTION
        | Self::SELECT_ONE
        | Self::INSERT
        | Self::UPDATE
        | Self::DELETE
        | Self::FILTER
        | Self::ORDER_BY;

    /// Table default: all flags enabled.
    pub fn table_defaults() -> Self {
        Self { flags: Self::ALL_FLAGS }
    }

    /// View default: CONNECTION | SELECT_ONE | FILTER | ORDER_BY (read-only).
    pub fn view_defaults() -> Self {
        Self {
            flags: Self::CONNECTION | Self::SELECT_ONE | Self::FILTER | Self::ORDER_BY,
        }
    }

    /// No flags set.
    pub fn none() -> Self {
        Self { flags: 0 }
    }

    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    pub fn add(&mut self, flag: u16) {
        self.flags |= flag;
    }

    pub fn remove(&mut self, flag: u16) {
        self.flags &= !flag;
    }

    /// Map a behavior name string to a flag value.
    pub fn flag_from_name(name: &str) -> Option<u16> {
        match name {
            "connection" | "many" => Some(Self::CONNECTION),
            "select" | "selectOne" => Some(Self::SELECT_ONE),
            "insert" | "create" => Some(Self::INSERT),
            "update" => Some(Self::UPDATE),
            "delete" => Some(Self::DELETE),
            "filter" => Some(Self::FILTER),
            "order" | "orderBy" => Some(Self::ORDER_BY),
            "all" => Some(Self::ALL_FLAGS),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_defaults_have_all_behaviors() {
        let b = BehaviorSet::table_defaults();
        assert!(b.has(BehaviorSet::CONNECTION));
        assert!(b.has(BehaviorSet::SELECT_ONE));
        assert!(b.has(BehaviorSet::INSERT));
        assert!(b.has(BehaviorSet::UPDATE));
        assert!(b.has(BehaviorSet::DELETE));
        assert!(b.has(BehaviorSet::FILTER));
        assert!(b.has(BehaviorSet::ORDER_BY));
    }

    #[test]
    fn test_view_defaults_are_read_only() {
        let b = BehaviorSet::view_defaults();
        assert!(b.has(BehaviorSet::CONNECTION));
        assert!(b.has(BehaviorSet::SELECT_ONE));
        assert!(!b.has(BehaviorSet::INSERT));
        assert!(!b.has(BehaviorSet::UPDATE));
        assert!(!b.has(BehaviorSet::DELETE));
        assert!(b.has(BehaviorSet::FILTER));
        assert!(b.has(BehaviorSet::ORDER_BY));
    }

    #[test]
    fn test_add_remove() {
        let mut b = BehaviorSet::none();
        assert!(!b.has(BehaviorSet::INSERT));
        b.add(BehaviorSet::INSERT);
        assert!(b.has(BehaviorSet::INSERT));
        b.remove(BehaviorSet::INSERT);
        assert!(!b.has(BehaviorSet::INSERT));
    }

    #[test]
    fn test_flag_from_name() {
        assert_eq!(BehaviorSet::flag_from_name("connection"), Some(BehaviorSet::CONNECTION));
        assert_eq!(BehaviorSet::flag_from_name("many"), Some(BehaviorSet::CONNECTION));
        assert_eq!(BehaviorSet::flag_from_name("select"), Some(BehaviorSet::SELECT_ONE));
        assert_eq!(BehaviorSet::flag_from_name("selectOne"), Some(BehaviorSet::SELECT_ONE));
        assert_eq!(BehaviorSet::flag_from_name("insert"), Some(BehaviorSet::INSERT));
        assert_eq!(BehaviorSet::flag_from_name("create"), Some(BehaviorSet::INSERT));
        assert_eq!(BehaviorSet::flag_from_name("update"), Some(BehaviorSet::UPDATE));
        assert_eq!(BehaviorSet::flag_from_name("delete"), Some(BehaviorSet::DELETE));
        assert_eq!(BehaviorSet::flag_from_name("filter"), Some(BehaviorSet::FILTER));
        assert_eq!(BehaviorSet::flag_from_name("order"), Some(BehaviorSet::ORDER_BY));
        assert_eq!(BehaviorSet::flag_from_name("orderBy"), Some(BehaviorSet::ORDER_BY));
        assert_eq!(BehaviorSet::flag_from_name("unknown"), None);
    }

    #[test]
    fn test_flag_from_name_all() {
        let all = BehaviorSet::flag_from_name("all").unwrap();
        let mut b = BehaviorSet { flags: all };
        assert!(b.has(BehaviorSet::CONNECTION));
        assert!(b.has(BehaviorSet::INSERT));
        assert!(b.has(BehaviorSet::DELETE));
        // Removing all disables everything
        b.remove(all);
        assert_eq!(b, BehaviorSet::none());
    }

    #[test]
    fn test_omit_all_disables_everything() {
        let mut b = BehaviorSet::table_defaults();
        // Simulate @omit all
        if let Some(flag) = BehaviorSet::flag_from_name("all") {
            b.remove(flag);
        }
        assert_eq!(b, BehaviorSet::none());
    }
}
