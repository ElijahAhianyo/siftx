use crate::field::FieldValue;

#[derive(Debug, Clone)]
pub struct SourceDocument{
    id: String,
    fields: Vec<FieldValue>
}

impl SourceDocument{
    pub fn new<T: IntoIterator<Item=FieldValue>>(id: String, fields: T) -> Self {
        Self{id, fields: fields.into_iter().collect()}
    }
}