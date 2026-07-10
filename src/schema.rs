use std::collections::HashMap;
use thiserror::Error;
use crate::field::{FieldEntry, FieldId, FieldType, TextOptions};

pub struct Schema{
    fields: Vec<FieldEntry>,
    field_map: HashMap<String, FieldId>,
}

impl Schema{
    pub fn new(fields: Vec<FieldEntry>, field_map: HashMap<String, FieldId>) -> Self{
        Self{
            fields,
            field_map,
        }
    }

    pub fn get_field_id<T: AsRef<str>>(&self, name: T) -> Option<FieldId> {
        self.field_map.get(name.as_ref()).cloned()
    }

    pub fn get_field_entry(&self, id: FieldId) -> Option<&FieldEntry> {
        self.fields.get(id.0 as usize)
    }

    pub fn get_field_entry_by_name<T: AsRef<str>>(&self, name: T) -> Option<&FieldEntry> {
        self.field_map.get(name.as_ref()).and_then(|id| self.get_field_entry(*id))
    }

    pub fn builder() -> SchemaBuilder {
        SchemaBuilder::new()
    }
}

#[derive(Debug, Clone, Error)]
pub enum SchemaError{
    #[error("unknown field id: {0:?}")]
    UnknownField(FieldId),

    #[error("type mismatch for field `{field}`")]
    TypeMismatch { field: String },

    #[error("duplicate field name: {0}")]
    DuplicateField(String),
}

#[derive(Debug, Clone)]
pub struct SchemaBuilder{
    fields: Vec<FieldEntry>,
    map: HashMap<String, FieldId>
}

impl SchemaBuilder{
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            map: HashMap::new()
        }
    }
    
    pub fn add_text_field(
        mut self,
        name: String,
        options: TextOptions,
        stored: bool,
        indexed: bool
    )-> Self {
        let id = FieldId::new(self.fields.len() as u64);
        
        let field_entry = FieldEntry::new(
            id,
            name.clone(),
            FieldType::Text(options),
            stored,
            indexed
        );
        self.map.insert(name, id);
        self.fields.push(field_entry);
        self
    }

    pub fn build(self) -> Schema {
        Schema::new(self.fields.clone(), self.map.clone())
    }
}