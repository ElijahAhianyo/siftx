use std::sync::Arc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::document::SourceDocument;
use crate::DocumentId;
use crate::error::SiftxError;
use crate::field::{FieldType, Value};
use crate::index::StoreDoc;
use crate::posting::{PostingsBuilder, Term};
use crate::schema::Schema;
use crate::tokenizer::{TokenStream, TokenizerManager};

#[derive(Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SegmentId(Uuid);

impl SegmentId{
    pub fn new() -> Self{
        Self(Uuid::new_v4())
    }
}


pub struct Segment{}
pub struct SegmentReader{}
pub struct SegmentWriter{
    schema: Arc<Schema>,
    postings: PostingsBuilder,
    tokenizer_manager: Arc<TokenizerManager>,
    max_doc: u32,
    store: Vec<StoreDoc>
}


impl SegmentWriter {
    pub fn new(schema: Arc<Schema>, tokenizer_manager: Arc<TokenizerManager>) -> Self {
        Self {
            schema,
            tokenizer_manager,
            max_doc: 0,
            store: vec![],
            postings: PostingsBuilder::new()
        }
    }

    pub fn add_document(&mut self, doc: &SourceDocument) -> crate::Result<()> {
        let doc_id = DocumentId(self.max_doc);
        self.max_doc += 1;

        for field_value in doc.fields(){
            let Some(entry) = self.schema.get_field_entry(field_value.field_id) else {
                continue
            };

            if entry.is_indexed() {
                continue
            }

            match (entry.field_type(), &field_value.value) {
                (FieldType::Text(options), Value::TEXT(text)) => {
                    let Some(mut analyzer) = self.tokenizer_manager.get(&options.analyzer) else {
                        return Err(SiftxError::UnknownTextAnalyzer(options.analyzer.clone()));
                    };

                    let mut token_stream = analyzer.token_stream(&text);
                    while let Some(token) =  token_stream.next() {
                        let term = Term {
                            text: token.term.clone(),
                            field: field_value.field_id
                        };
                        self.postings.record(term, doc_id, token.position as u32)
                    }

                }
                _ => {}
            }
        };
        self.store.push(StoreDoc::from_source(doc, self.schema.clone()));
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMeta{}
