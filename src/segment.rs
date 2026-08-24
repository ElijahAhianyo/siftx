use crate::DocumentId;
use crate::directory::Directory;
use crate::document::SourceDocument;
use crate::error::SiftxError;
use crate::field::{FieldType, Value};
use crate::index::StoreDoc;
use crate::posting::{PostingList, PostingsBuilder, Term, TermEntry};
use crate::schema::Schema;
use crate::tokenizer::{TokenStream, TokenizerManager};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;
use wincode::{SchemaRead, SchemaWrite};
#[derive(
    Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Serialize, Deserialize, SchemaWrite, SchemaRead,
)]
pub struct SegmentId(Uuid);

impl SegmentId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Display for SegmentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_simple().to_string())
    }
}

pub struct Segment {
    segment_id: SegmentId,
    max_doc: u32,
    term_infos: Vec<(Term, TermEntry)>,
    store: Vec<StoreDoc>,
    dir: Arc<dyn Directory>,
}

impl Segment {
    pub fn term_entry(&self, term: &Term) -> Option<&TermEntry> {
        self.term_infos
            .binary_search_by(|(t, _)| t.cmp(term))
            .map(|i| &self.term_infos[i].1)
            .ok()
    }

    pub fn term_infos(&self) -> &[(Term, TermEntry)] {
        &self.term_infos
    }
    pub fn postings(&self, term: &Term) -> crate::Result<Option<PostingList>> {
        let Some(term_entry) = self.term_entry(term) else {
            return Ok(None);
        };
        let start = term_entry.offset as u64;
        let end = start + term_entry.len as u64;
        let segment_id = format!("{}.post", self.segment_id.to_string());
        let path = Path::new(&segment_id);
        let bytes = self.dir.read_range(path, start..end)?;
        Ok(Some(wincode::deserialize(&bytes).unwrap()))
    }

    pub fn max_id(&self) -> u32 {
        self.max_doc
    }

    pub fn segment_id(&self) -> &SegmentId {
        &self.segment_id
    }
}
pub struct SegmentReader {
    dir: Arc<dyn Directory>,
    meta: SegmentMeta,
}

impl SegmentReader {
    pub fn new(meta: SegmentMeta, dir: Arc<dyn Directory>) -> Self {
        Self { meta, dir }
    }

    pub fn open(&self) -> crate::Result<Segment> {
        let term_infos = wincode::deserialize(&self.dir.read(Path::new(&format!(
            "{}.term",
            self.meta.segment_id.to_string()
        )))?)
        .unwrap();

        let store = wincode::deserialize(&self.dir.read(Path::new(&format!(
            "{}.store",
            self.meta.segment_id.to_string()
        )))?)
        .unwrap();

        Ok(Segment {
            segment_id: self.meta.segment_id.clone(),
            max_doc: self.meta.max_doc,
            term_infos,
            store,
            dir: self.dir.clone(),
        })
    }
}

pub struct SegmentWriter {
    schema: Arc<Schema>,
    postings: PostingsBuilder,
    tokenizer_manager: Arc<TokenizerManager>,
    max_doc: u32,
    store: Vec<StoreDoc>,
}

impl SegmentWriter {
    pub fn new(schema: Arc<Schema>, tokenizer_manager: Arc<TokenizerManager>) -> Self {
        Self {
            schema,
            tokenizer_manager,
            max_doc: 0,
            store: vec![],
            postings: PostingsBuilder::new(),
        }
    }

    pub fn add_document(&mut self, doc: &SourceDocument) -> crate::Result<()> {
        let doc_id = DocumentId(self.max_doc);
        self.max_doc += 1;

        for field_value in doc.fields() {
            let Some(entry) = self.schema.get_field_entry(field_value.field_id) else {
                continue;
            };

            if !entry.should_index() {
                continue;
            }

            match (entry.field_type(), &field_value.value) {
                (FieldType::Text(options), Value::TEXT(text)) => {
                    let Some(mut analyzer) = self.tokenizer_manager.get(&options.analyzer).cloned()
                    else {
                        return Err(SiftxError::UnknownTextAnalyzer(options.analyzer.clone()));
                    };

                    let mut token_stream = analyzer.token_stream(&text);
                    while let Some(token) = token_stream.next() {
                        let term = Term {
                            text: token.term.clone(),
                            field: field_value.field_id,
                        };
                        self.postings.record(term, doc_id, token.position as u32)
                    }
                }
                _ => {}
            }
        }
        self.store
            .push(StoreDoc::from_source(doc, self.schema.clone()));
        Ok(())
    }

    pub fn finalize(self, dir: &dyn Directory) -> crate::Result<SegmentMeta> {
        let segment_id = SegmentId::generate();
        let mut term_infos: Vec<(Term, TermEntry)> = Vec::new();
        let mut postings_blob: Vec<u8> = Vec::new();

        for (term, posting_list) in self.postings {
            let doc_freq = posting_list.doc_freq();
            let offset = postings_blob.len();
            let encoded = wincode::serialize(&posting_list).unwrap();
            term_infos.push((
                term,
                TermEntry {
                    offset,
                    doc_freq,
                    len: encoded.len(),
                },
            ));
            postings_blob.extend(encoded);
        }

        dir.write(
            Path::new(&format!("{}.term", segment_id.to_string())),
            &wincode::serialize(&term_infos).unwrap(),
        )?;

        dir.write(
            Path::new(&format!("{}.post", segment_id.to_string())),
            &postings_blob,
        )?;

        dir.write(
            Path::new(&format!("{}.store", segment_id.to_string())),
            &wincode::serialize(&self.store).unwrap(),
        )?;

        Ok(SegmentMeta {
            segment_id,
            max_doc: self.max_doc,
        })
    }

    pub fn memory_usage(&self) -> usize {
        self.postings.memory_usage()
    }

    pub fn max_doc(&self) -> u32 {
        self.max_doc
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub struct SegmentMeta {
    pub segment_id: SegmentId,
    pub max_doc: u32,
}
