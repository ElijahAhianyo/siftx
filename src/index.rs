use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use serde::{Deserialize, Serialize};
use crate::directory::{Directory, FsDirectory};
use crate::document::SourceDocument;
use crate::schema::Schema;
use crate::tokenizer::TokenizerManager;
use crate::Result;

struct IndexReader{}

#[derive(Debug, Clone,Default, Serialize, Deserialize )]
struct IndexMeta{
    segments: Vec<SegmentMeta>,
    next_segment_id: u64,
}

struct Segment{}
struct SegmentReader{}
struct SegmentWriter{}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SegmentMeta{}

const META_FILE: &str = "meta.json";

#[derive(Clone)]
struct Index {
    dir: Arc<dyn Directory>,
    schema: Arc<Schema>,
    tokenizers: TokenizerManager
}

impl Index {

    pub fn create_in_dir(path: &Path, schema: Schema) -> Result<Self> {
        let dir = FsDirectory::open(path)?;
        Self::bootstrap(dir, schema)
    }

    fn bootstrap<D: Directory + Sync + 'static>(dir: D, schema: Schema) -> Result<Self>{
        let dir = Arc::new(dir);
        if !dir.exists(Path::new(META_FILE)){
            dir.write(Path::new(META_FILE), wincode::serialize(&IndexMeta::default()).unwrap().as_slice())?;
        }

        Ok(Self{
            dir: Arc::new(dir),
            schema: Arc::new(schema),
            tokenizers: TokenizerManager::default()
        })
    }

    pub fn writer(&self) -> Result<IndexWriter>{
       let writer =  IndexWriter{
            index: self.clone(),
            segment_writer: None,
            meta: self.load_meta()?,
            memory_budget_bytes: 64 * 1024 * 1024, // 64MB
        };
        Ok(writer)
    }

    pub fn load_meta(&self) -> Result<IndexMeta>{
        Ok(wincode::deserialize(&self.dir.read(Path::new(META_FILE)).unwrap()).unwrap())
    }
}


struct IndexWriter{
    index: Index,
    segment_writer: Option<SegmentWriter>,
    meta: IndexMeta,
    memory_budget_bytes: usize,
}

impl IndexWriter{
    pub fn add_document(&self, document: SourceDocument) -> Result<()>{
        
    }
}