use crate::Result;
use crate::directory::{Directory, FsDirectory};
use crate::document::SourceDocument;
use crate::field::FieldValue;
use crate::schema::Schema;
use crate::segment::{SegmentMeta, SegmentReader, SegmentWriter};
use crate::tokenizer::TokenizerManager;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use wincode::{SchemaRead, SchemaWrite};

struct IndexReader {
    segments: Vec<SegmentReader>,
    schema: Arc<Schema>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub struct IndexMeta {
    segments: Vec<SegmentMeta>,
    next_segment_id: u64,
}

#[derive(Debug, Clone, SchemaRead, SchemaWrite)]
pub struct StoreDoc {
    id: String,
    fields: Vec<FieldValue>,
}

impl StoreDoc {
    pub fn from_source(doc: &SourceDocument, schema: Arc<Schema>) -> Self {
        let fields = doc
            .fields()
            .iter()
            .filter(|f| {
                schema
                    .get_field_entry(f.field_id)
                    .map(|f| f.is_indexed())
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        Self {
            id: doc.id().clone(),
            fields,
        }
    }
}

const META_FILE: &str = "meta.json";

#[derive(Clone)]
pub struct Index {
    dir: Arc<dyn Directory>,
    schema: Arc<Schema>,
    tokenizers: Arc<TokenizerManager>,
}

impl Index {
    pub fn create_in_dir(path: &Path, schema: Schema) -> Result<Self> {
        let dir = FsDirectory::open(path)?;
        Self::bootstrap(dir, schema)
    }

    fn bootstrap<D: Directory + 'static>(dir: D, schema: Schema) -> Result<Self> {
        let dir = Arc::new(dir);
        if !dir.exists(Path::new(META_FILE)) {
            dir.write(
                Path::new(META_FILE),
                wincode::serialize(&IndexMeta::default())
                    .unwrap()
                    .as_slice(),
            )?;
        }

        Ok(Self {
            dir,
            schema: Arc::new(schema),
            tokenizers: Arc::new(TokenizerManager::default()),
        })
    }

    pub fn writer(&self) -> Result<IndexWriter> {
        let memory_budget_bytes = 64 * 1024 * 1024; // 64MB
        let headroom = 0.10;
        let flush_threshold_bytes =
            memory_budget_bytes - (memory_budget_bytes as f64 * headroom) as usize;
        let writer = IndexWriter {
            index: self.clone(),
            segment_writer: None,
            meta: self.load_meta()?,
            memory_budget_bytes,
            flush_threshold_bytes,
        };
        Ok(writer)
    }

    pub fn load_meta(&self) -> Result<IndexMeta> {
        Ok(wincode::deserialize(&self.dir.read(Path::new(META_FILE))?).unwrap())
    }
}

pub struct IndexWriter {
    index: Index,
    segment_writer: Option<SegmentWriter>,
    meta: IndexMeta,
    memory_budget_bytes: usize,
    flush_threshold_bytes: usize,
}

impl IndexWriter {
    pub fn add_document(&mut self, document: SourceDocument) -> Result<()> {
        if let Some(segment_writer) = &self.segment_writer
            && segment_writer.memory_usage() >= self.flush_threshold_bytes
        {
            self.flush_current_segment()?;
        }

        if self.segment_writer.is_none() {
            self.segment_writer = Some(SegmentWriter::new(
                self.index.schema.clone(),
                self.index.tokenizers.clone(),
            ));
        }

        let mut writer = self
            .segment_writer
            .as_mut()
            .expect("should be initialized at this point");
        writer.add_document(&document)?;

        // flush immediately again if adding the document surpassed the actual memory ceiling.
        if writer.memory_usage() >= self.memory_budget_bytes {
            self.flush_current_segment()?;
        }
        Ok(())
    }

    pub fn flush_current_segment(&mut self) -> Result<()> {
        if let Some(writer) = self.segment_writer.take()
            && writer.max_doc() > 0
        {
            let seg_meta = writer.finalize(self.index.dir.as_ref())?;
            self.meta.segments.push(seg_meta);
        }
        Ok(())
    }

    pub fn commit(&mut self) -> Result<()> {
        self.flush_current_segment()?;
        let data = wincode::serialize(&self.meta).unwrap();
        self.index.dir.write(Path::new(META_FILE), &data)?;
        Ok(())
    }
}
