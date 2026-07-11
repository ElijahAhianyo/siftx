use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::str::CharIndices;

mod query;
mod document;
mod posting;
mod token;
pub mod schema;
pub mod field;
mod tokenizer;
mod index;
mod directory;
mod error;
mod segment;

use thiserror::Error;
use bumpalo::{Bump};
use crate::document::SourceDocument;
use crate::field::{FieldValue, Value};
use crate::schema::Schema;
use crate::token::Token;
use crate::error::SiftxError;


#[derive(Debug, Error)]
pub enum Error{
    #[error("IO error")]
    Io(#[from] std::io::Error),
}
pub type Result<T> = std::result::Result<T, SiftxError>;
pub struct Directory{
    path: PathBuf
}

impl Directory{
    pub fn new(path: PathBuf) -> Self {
        Self{path}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct DocumentId(u32);

#[derive(Debug)]
pub struct Document{
    path: PathBuf,
    doc_id: DocumentId
}

impl Document{
    pub fn new(path: PathBuf) -> Self {
        Self{path, doc_id: DocumentId(0)}
    }

    pub fn open(&self) -> Result<ContentChunk>{
        let reader = DocumentLoader::open_reader(self)?;
        Ok(ContentChunk::new(reader))
    }
}

// impl ToBytes for DocumentId{
//     type Bytes = Vec<u8>;
//     fn to_le_bytes(&self) -> Self::Bytes {
//         self.0.to_le_bytes().into()
//     }
//
//     fn to_be_bytes(&self) -> Self::Bytes {
//         self.0.to_be_bytes().into()
//     }
// }
//
//
// impl FromBytes for DocumentId{
//     type Bytes = [u8; 8];
//     fn from_le_bytes(bytes: Self::Bytes) -> Self {
//         Self(u64::from_le_bytes(bytes))
//     }
//
//     fn from_be_bytes(bytes: Self::Bytes) -> Self {
//         Self(u64::from_be_bytes(bytes))
//     }
// }

pub enum FsEntry{
    Dir(Directory),
    File(Document)
}


impl FsEntry{
    pub fn from_path(path: PathBuf) -> Self {
        if path.is_dir(){
            Self::Dir(Directory::new(path))
        }
        else {
            Self::File(Document::new(path))
        }
    }
}


pub struct FileWalker{
    stack: Vec<PathBuf>
}

impl FileWalker{
    pub fn new(entry: FsEntry) -> Self {
        match entry {
            FsEntry::Dir(dir) => Self{stack: vec![dir.path]},
            FsEntry::File(doc) => Self{stack: vec![doc.path]}
        }
    }
}

impl Iterator for FileWalker{
    type Item = Result<Document>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(path) = self.stack.pop(){
            if path.is_dir(){
                match std::fs::read_dir(path){
                    Ok(entries) => {
                        for e in entries.flatten(){
                            self.stack.push(e.path());
                        }
                    }
                    Err(e) => return Some(Err(e.into()))
                }
            }
            else {
                return Some(Ok(Document::new(path)))
            }
        }
        None
    }
}

pub trait ContentReader{
    type Chunk: AsRef<str>;
    fn next_chunk(&mut self) -> Result<Option<Self::Chunk>>;
}


struct PDFReader<R: Read>{
    doc: lopdf::Document,
    page_nums: Vec<u32>,
    cursor: usize,
    phantom: std::marker::PhantomData<R>,
}

impl<R: Read> PDFReader<R>{
    pub fn new(mut reader: R) -> Result<Self>{
        let mut raw = Vec::new();
        reader.read_to_end(&mut raw)?;

        let doc = lopdf::Document::load_mem(&raw).unwrap();
        let page_nums = doc.get_pages().keys().cloned().collect();
        Ok(Self{doc, page_nums, cursor: 0, phantom: std::marker::PhantomData})
    }
}

struct CSVReader<R: Read>{
    inner: csv::Reader<R>,
    record: csv::StringRecord,
}

impl <R: Read> CSVReader<R> {
    pub fn new(reader: R) -> Self {
        Self{
            inner: csv::ReaderBuilder::new().has_headers(true).flexible(true).from_reader(reader),
            record: csv::StringRecord::new(),
        }
    }
}

struct PlaintextReader<R: BufRead>{
    inner: R,
    buf: String
}

impl<R: BufRead> PlaintextReader<R>{
    pub fn new(reader: R) -> Self {
        Self{ inner: reader, buf: String::new()}
    }
}

impl <R: Read> ContentReader for PDFReader<R>{
    type Chunk = String;
    fn next_chunk(&mut self) -> Result<Option<Self::Chunk>>{
        if self.cursor >= self.page_nums.len() {
            return Ok(None);
        }

        let page_num = self.page_nums[self.cursor];
        self.cursor += 1;

        let text = self.doc.extract_text(&[page_num]).unwrap_or_default();
        Ok(Some(text))
    }
}
impl <R: Read> ContentReader for CSVReader<R>{
    type Chunk = String;
    fn next_chunk(&mut self) -> Result<Option<Self::Chunk>> {
        if self.inner.read_record(&mut self.record).unwrap(){
            Ok(Some(self.record.iter().collect::<Vec<_>>().join(" ")))
        } else {
            Ok(None)
        }
    }
}

impl <R: BufRead> ContentReader for PlaintextReader<R>{
    type Chunk = String;
    fn next_chunk(&mut self) -> Result<Option<Self::Chunk>> {
        self.buf.clear();
        match self.inner.read_line(&mut self.buf){
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(self.buf.trim().to_string())),
            Err(e) => Err(e.into())
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

pub struct ContentChunk{
    reader: AnyReader,
}

impl ContentChunk{
    pub fn new(reader: AnyReader) -> Self {
        Self{reader}
    }
}

impl Iterator for ContentChunk{
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        self.reader.next_chunk().transpose()
    }
}

struct DocumentLoader;

impl DocumentLoader{
    pub fn open_reader(document: &Document) -> Result<AnyReader>{
        let ext = document.path.extension().unwrap().to_str().unwrap();
        let file = std::fs::File::open(document.path.clone())?;
        match ext{
            "pdf" => Ok(AnyReader::PDF(PDFReader::new(file)?)),
            "csv" => Ok(AnyReader::CSV(CSVReader::new(file))),
            _ => Ok(AnyReader::Plaintext(PlaintextReader::new(std::io::BufReader::new(file)))),
        }
    }
}


pub struct FileDocumentSource{
    walker: FileWalker,
    schema: Schema,
}

impl FileDocumentSource{
    pub fn new(path: PathBuf, schema: Schema) -> Self {
        Self{walker: FileWalker::new(FsEntry::from_path(path)), schema}
    }
}

impl Iterator for FileDocumentSource{
    type Item = Result<SourceDocument>;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(document) = self.walker.next() else {
            return None;
        };

        if let Ok(document) = document{
            let mut body = String::new();

            for chunk in document.open().unwrap(){
                match chunk{
                    Ok(text) => {
                        body.push_str(&text);
                        body.push('\n');
                    },
                    Err(e) => return Some(Err(e))
                }
            };

            let source_doc = SourceDocument::new(
                document.path.to_str()?.to_string(),
                vec![
                    FieldValue {
                        field_id: self.schema.get_field_id("body")?, //TODO: add type checking
                        value: Value::TEXT(body),
                    }
                ]
            );
            Some(Ok(source_doc))

        } else {
            Some(Err(document.unwrap_err()))
        }
    }
}

// #[derive(Debug, Clone)]
// struct Token{
//     term: String,
//     position: usize,
//     start_offset: usize,
//     end_offset: usize,
// }
//
// impl Token{
//     pub fn new() -> Self {
//         Self{
//             term: String::new(),
//             position: usize::MAX,
//             start_offset: 0,
//             end_offset: 0,
//         }
//     }
//
//     pub fn clear(&mut self){
//         self.term.clear();
//         self.position = usize::MAX;
//         self.start_offset = 0;
//         self.end_offset = 0;
//     }
// }
//
// impl Default for Token{
//     fn default() -> Self {
//         Self::new()
//     }
// }
//
//
// trait TokenStream{
//     fn advance(&mut self) -> bool;
//     fn token(&self) -> &Token;
//     fn token_mut(&mut self) -> &mut Token;
//     fn next(&mut self) -> Option<&Token>{
//         if self.advance(){
//             Some(self.token())
//         } else {
//             None
//         }
//     }
//
// }
//
// trait Tokenizer {
//     type TokenStream<'a>: TokenStream where Self: 'a;
//     fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a>;
// }
//
//
// struct BasicTokenizer{
//     token: Token
// }
//
// impl Tokenizer for BasicTokenizer{
//     type TokenStream<'a> = BasicTokenizerStream<'a>;
//
//     fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
//         self.token.clear();
//         BasicTokenizerStream{
//             token: &mut self.token,
//             chars: text.char_indices(),
//             text
//         }
//     }
// }
//
// struct BasicTokenizerStream<'a>{
//     token: &'a mut Token,
//     chars: CharIndices<'a>,
//     text: &'a str,
// }
//
// impl BasicTokenizerStream<'_>{
//     fn find_token_end( &mut self) -> usize {
//         (&mut self.chars).filter(|(_, c)| !c.is_alphanumeric())
//             .map(|(i, _)| i)
//             .next().unwrap_or(self.text.len())
//     }
// }
//
// impl<'a> TokenStream for BasicTokenizerStream<'a>{
//     fn advance(&mut self) -> bool {
//         self.token.term.clear();
//         self.token.position = self.token.position.wrapping_add(1);
//
//         while let Some((i, c)) = self.chars.next(){
//             if c.is_alphanumeric(){
//                 let end_offset = self.find_token_end();
//                 self.token.term.push_str(&self.text[i..end_offset]);
//                 self.token.start_offset = i;
//                 self.token.end_offset = end_offset;
//                 return true;
//             }
//         }
//         false
//     }
//
//     fn token(&self) -> &Token {
//         &self.token
//     }
//
//     fn token_mut(&mut self) -> &mut Token {
//         &mut self.token
//     }
// }
//
// pub trait BoxedTokenizer{
//     fn box_token_stream(&mut self, text: &str) -> BoxedTokenStream;
// }
//
// impl Tokenizer for Box<dyn BoxedTokenizer>{
//     type TokenStream<'a> = BoxedTokenStream<'a>;
//
//     fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
//         todo!()
//     }
// }
//
// pub struct BoxedTokenStream<'a>(Box<dyn TokenStream + 'a>);
//
// impl<'a> TokenStream for BoxedTokenStream<'a>{
//     fn advance(&mut self) -> bool {
//         self.0.advance()
//     }
//
//     fn token(&self) -> &Token {
//         self.0.token()
//     }
//
//     fn token_mut(&mut self) -> &mut Token {
//         self.0.token_mut()
//     }
// }
//
// pub struct TokenManager{
//     tokenizers: Box<dyn Tokenizer>
// }

#[derive(Debug, Clone)]
struct Posting{
    doc_id: DocumentId,
    positions: Vec<usize>,
}

// impl ToBytes for Posting{
//     type Bytes = Vec<u8>;
//
//     fn to_le_bytes(&self) -> Self::Bytes {
//         let doc_bytes = self.doc_id.to_le_bytes();
//     }
//
//     fn to_be_bytes(&self) -> Self::Bytes {
//         todo!()
//     }
// }

// #[derive(Debug, Clone)]
// struct Index{
//     postings: BTreeMap<String, Vec<Posting>>
// }
//
// impl Index{
//     pub fn new() -> Self {
//         Self{postings: BTreeMap::new()}
//     }
//
//      fn insert(&mut self, token: &Token, doc_id: DocumentId){
//         let entry = self.postings.entry(token.term.to_string()).or_insert_with(Vec::new);
//
//          match entry.binary_search_by_key(&doc_id, |p| p.doc_id){
//              Ok(i) => entry[i].positions.push(token.position),
//              Err(i) => entry.insert(i, Posting{doc_id, positions: vec![token.position]})
//          }
//     }
//
//     pub fn reset() -> Self{
//         Self::new()
//     }
//
//     pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<Posting>)>{
//         self.postings.iter()
//     }
//
//     pub fn iter_mut(&mut self) -> impl Iterator<Item = (&String, &mut Vec<Posting>)>{
//         self.postings.iter_mut()
//     }
// }

// struct SegmentId(u64);
//
// impl SegmentId{
//     pub fn new() -> Self{
//         Self(u64::MAX)
//     }
//
//     pub fn next(&mut self) -> Self{
//         self.0 += 1;
//         Self(self.0)
//     }
// }

// struct SegmentMetadata{
//     segment_id: SegmentId,
//     path: PathBuf,
//     doc_count: u64,
//     term_count: u64,
// }
//
// struct IndexWriter{
//     index: Index,
//     flush_threshold: usize,
//     segment_count: u64,
//     output_dir: PathBuf,
//     current_segment_id: SegmentId,
// }
//
// impl IndexWriter{
//     pub fn new(flush_threshold: usize, output_dir: PathBuf) -> Self {
//         Self{
//             index: Index::new(),
//             flush_threshold,
//             segment_count: 0,
//             output_dir,
//             current_segment_id: SegmentId::new(),
//         }
//     }
//
//     pub fn add_token(&mut self, token: &Token, doc_id: DocumentId){
//         self.index.insert(token, doc_id);
//     }
//
//     pub fn flush(&mut self) -> Result<SegmentMetadata>{
//         self.segment_count += 1;
//
//         let segment_id = self.current_segment_id.next();
//         let path = self.output_dir.join(format!("segment-{}.bin", segment_id.0));
//         let meta = SegmentWriter::write(&mut self.index, path, segment_id)?;
//         self.index = Index::new();
//         Ok(meta)
//     }
// }

// struct SegmentWriter;
//
// impl SegmentWriter{
//     pub fn write(index: &mut Index, path: PathBuf, segment_id: SegmentId) -> Result<SegmentMetadata>{
//         let file = std::fs::File::create(path.clone())?;
//         let mut writer = std::io::BufWriter::new(file);
//         let mut doc_count = 0;
//
//         for (term, postings) in index.iter_mut() {
//             postings.sort_unstable_by_key(|p| p.doc_id);
//             postings.dedup_by_key(|p| p.doc_id);
//             doc_count += postings.len();
//
//             let term_bytes = term.as_bytes();
//             writer.write_all(&(term_bytes.len() as u32).to_le_bytes())?;
//             writer.write_all(term_bytes)?;
//
//             let postings_len = postings.len();
//             writer.write_all(&(postings_len as u32).to_le_bytes())?;
//             for posting in postings {
//                 // writer.write_all(&(posting.to_le_bytes()))?;
//             }
//         }
//         writer.flush()?;
//         let meta = SegmentMetadata{
//             segment_id,
//             path,
//             doc_count: doc_count as u64,
//             term_count: index.postings.len() as u64
//         };
//         Ok(meta)
//     }
// }


// trait ToBytes {
//     type Bytes: AsRef<[u8]>;
//     fn to_le_bytes(&self) -> Self::Bytes;
//     fn to_be_bytes(&self) -> Self::Bytes;
//
// }
//
//
// trait FromBytes {
//     type Bytes: AsRef<[u8]>;
//     fn from_le_bytes(bytes: Self::Bytes) -> Self;
//     fn from_be_bytes(bytes: Self::Bytes) -> Self;
// }