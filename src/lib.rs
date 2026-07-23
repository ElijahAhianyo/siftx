use std::io::{BufRead, Read, Write};
use std::path::PathBuf;

mod arena;
pub mod directory;
pub mod document;
pub mod error;
pub mod field;
pub mod index;
pub mod posting;
pub mod query;
pub mod schema;
pub mod segment;
pub mod token;
pub mod tokenizer;

use crate::document::SourceDocument;
use crate::error::SiftxError;
use crate::field::{FieldValue, Value};
use crate::schema::Schema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wincode::{SchemaRead, SchemaWrite};

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error")]
    Io(#[from] std::io::Error),
}
pub type Result<T> = std::result::Result<T, SiftxError>;
pub struct Directory {
    path: PathBuf,
}

impl Directory {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
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
pub struct DocumentId(u32);

#[derive(Debug)]
pub struct Document {
    path: PathBuf,
    doc_id: DocumentId,
}

impl Document {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            doc_id: DocumentId(0),
        }
    }

    pub fn open(&self) -> Result<ContentChunk> {
        let reader = DocumentLoader::open_reader(self)?;
        Ok(ContentChunk::new(reader))
    }
}

pub enum FsEntry {
    Dir(Directory),
    File(Document),
}

impl FsEntry {
    pub fn from_path(path: PathBuf) -> Self {
        if path.is_dir() {
            Self::Dir(Directory::new(path))
        } else {
            Self::File(Document::new(path))
        }
    }
}

pub struct FileWalker {
    stack: Vec<PathBuf>,
}

impl FileWalker {
    pub fn new(entry: FsEntry) -> Self {
        match entry {
            FsEntry::Dir(dir) => Self {
                stack: vec![dir.path],
            },
            FsEntry::File(doc) => Self {
                stack: vec![doc.path],
            },
        }
    }
}

impl Iterator for FileWalker {
    type Item = Result<Document>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(path) = self.stack.pop() {
            if path.is_dir() {
                match std::fs::read_dir(path) {
                    Ok(entries) => {
                        for e in entries.flatten() {
                            self.stack.push(e.path());
                        }
                    }
                    Err(e) => return Some(Err(e.into())),
                }
            } else {
                return Some(Ok(Document::new(path)));
            }
        }
        None
    }
}

pub trait ContentReader {
    type Chunk: AsRef<str>;
    fn next_chunk(&mut self) -> Result<Option<Self::Chunk>>;
}

struct PDFReader<R: Read> {
    doc: lopdf::Document,
    page_nums: Vec<u32>,
    cursor: usize,
    phantom: std::marker::PhantomData<R>,
}

impl<R: Read> PDFReader<R> {
    pub fn new(mut reader: R) -> Result<Self> {
        let mut raw = Vec::new();
        reader.read_to_end(&mut raw)?;

        let doc = lopdf::Document::load_mem(&raw).unwrap();
        let page_nums = doc.get_pages().keys().cloned().collect();
        Ok(Self {
            doc,
            page_nums,
            cursor: 0,
            phantom: std::marker::PhantomData,
        })
    }
}

struct CSVReader<R: Read> {
    inner: csv::Reader<R>,
    record: csv::StringRecord,
}

impl<R: Read> CSVReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            inner: csv::ReaderBuilder::new()
                .has_headers(true)
                .flexible(true)
                .from_reader(reader),
            record: csv::StringRecord::new(),
        }
    }
}

struct PlaintextReader<R: BufRead> {
    inner: R,
    buf: String,
}

impl<R: BufRead> PlaintextReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            inner: reader,
            buf: String::new(),
        }
    }
}

impl<R: Read> ContentReader for PDFReader<R> {
    type Chunk = String;
    fn next_chunk(&mut self) -> Result<Option<Self::Chunk>> {
        if self.cursor >= self.page_nums.len() {
            return Ok(None);
        }

        let page_num = self.page_nums[self.cursor];
        self.cursor += 1;

        let text = self.doc.extract_text(&[page_num]).unwrap_or_default();
        Ok(Some(text))
    }
}
impl<R: Read> ContentReader for CSVReader<R> {
    type Chunk = String;
    fn next_chunk(&mut self) -> Result<Option<Self::Chunk>> {
        if self.inner.read_record(&mut self.record).unwrap() {
            Ok(Some(self.record.iter().collect::<Vec<_>>().join(" ")))
        } else {
            Ok(None)
        }
    }
}

impl<R: BufRead> ContentReader for PlaintextReader<R> {
    type Chunk = String;
    fn next_chunk(&mut self) -> Result<Option<Self::Chunk>> {
        self.buf.clear();
        match self.inner.read_line(&mut self.buf) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(self.buf.trim().to_string())),
            Err(e) => Err(e.into()),
        }
    }
}

pub enum AnyReader {
    PDF(PDFReader<std::fs::File>),
    CSV(CSVReader<std::fs::File>),
    Plaintext(PlaintextReader<std::io::BufReader<std::fs::File>>),
}

impl AnyReader {
    pub fn next_chunk(&mut self) -> Result<Option<String>> {
        match self {
            AnyReader::Plaintext(pt) => pt.next_chunk(),
            AnyReader::CSV(csv) => csv.next_chunk(),
            AnyReader::PDF(pdf) => pdf.next_chunk(),
        }
    }
}

pub struct ContentChunk {
    reader: AnyReader,
}

impl ContentChunk {
    pub fn new(reader: AnyReader) -> Self {
        Self { reader }
    }
}

impl Iterator for ContentChunk {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        self.reader.next_chunk().transpose()
    }
}

struct DocumentLoader;

impl DocumentLoader {
    pub fn open_reader(document: &Document) -> Result<AnyReader> {
        let ext = document.path.extension().unwrap().to_str().unwrap();
        let file = std::fs::File::open(document.path.clone())?;
        match ext {
            "pdf" => Ok(AnyReader::PDF(PDFReader::new(file)?)),
            "csv" => Ok(AnyReader::CSV(CSVReader::new(file))),
            _ => Ok(AnyReader::Plaintext(PlaintextReader::new(
                std::io::BufReader::new(file),
            ))),
        }
    }
}

pub struct FileDocumentSource {
    walker: FileWalker,
    schema: Schema,
}

impl FileDocumentSource {
    pub fn new(path: PathBuf, schema: Schema) -> Self {
        Self {
            walker: FileWalker::new(FsEntry::from_path(path)),
            schema,
        }
    }
}

impl Iterator for FileDocumentSource {
    type Item = Result<SourceDocument>;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(document) = self.walker.next() else {
            return None;
        };

        if let Ok(document) = document {
            let mut body = String::new();

            for chunk in document.open().unwrap() {
                match chunk {
                    Ok(text) => {
                        body.push_str(&text);
                        body.push('\n');
                    }
                    Err(e) => return Some(Err(e)),
                }
            }

            let source_doc = SourceDocument::new(
                document.path.to_str()?.to_string(),
                vec![FieldValue {
                    field_id: self.schema.get_field_id("body")?, //TODO: add type checking
                    value: Value::TEXT(body),
                }],
            );
            Some(Ok(source_doc))
        } else {
            Some(Err(document.unwrap_err()))
        }
    }
}
