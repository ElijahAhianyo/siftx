use serde::{Deserialize, Serialize};
use wincode::{SchemaRead, SchemaWrite};

#[derive(Debug, Clone)]
pub struct FieldEntry {
    field_id: FieldId,
    name: String,
    field_type: FieldType,
    stored: bool,
    indexed: bool,
}

impl FieldEntry {
    pub fn new(
        field_id: FieldId,
        name: String,
        field_type: FieldType,
        stored: bool,
        indexed: bool,
    ) -> Self {
        Self {
            field_id,
            field_type,
            name,
            stored,
            indexed,
        }
    }

    pub fn is_indexed(&self) -> bool {
        self.indexed
    }

    pub fn field_type(&self) -> &FieldType {
        &self.field_type
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
    SchemaRead,
    SchemaWrite,
)]
pub struct FieldId(pub u64);

impl FieldId {
    pub fn new(val: u64) -> Self {
        Self(val)
    }
}

#[derive(Debug, Clone, SchemaRead, SchemaWrite)]
pub struct FieldValue {
    pub field_id: FieldId,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub enum FieldType {
    Text(TextOptions),
    Bool,
}

#[derive(Debug, Clone)]
pub struct TextOptions {
    pub analyzer: String,
    pub record: Record,
    pub field_norms: bool,
}

#[derive(Debug, Clone, Default)]
pub enum Record {
    #[default]
    Basic,
    WithFreqs,
    WithFreqsAndPos,
}

#[derive(Debug, Clone, SchemaWrite, SchemaRead)]
pub enum Value {
    TEXT(String),
}
