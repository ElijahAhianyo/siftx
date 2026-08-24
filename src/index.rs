use crate::Result;
use crate::directory::{Directory, FileLockGuard, FsDirectory};
use crate::document::SourceDocument;
use crate::field::FieldValue;
use crate::posting::{DocPosting, Term, TermEntry};
use crate::schema::Schema;
use crate::segment::{Segment, SegmentId, SegmentMeta, SegmentReader, SegmentWriter};
use crate::tokenizer::TokenizerManager;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::BufReader;
use std::path::Path;
use std::slice::Iter;
use std::sync::Arc;
use wincode::{SchemaRead, SchemaWrite};

pub struct IndexReader {
    segments: Vec<SegmentReader>,
    schema: Arc<Schema>,
}

impl IndexReader {
    pub fn open(
        index_meta: &IndexMeta,
        schema: Arc<Schema>,
        dir: Arc<dyn Directory>,
    ) -> Result<Self> {
        let segments = index_meta
            .segments
            .iter()
            .map(|s| SegmentReader::new(s.clone(), dir.clone()))
            .collect::<Vec<_>>();
        Ok(Self { segments, schema })
    }

    pub fn segments(&self) -> &[SegmentReader] {
        &self.segments
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, SchemaWrite, SchemaRead)]
pub struct IndexMeta {
    segments: Vec<SegmentMeta>,
    delete_queue: VecDeque<SegmentId>,
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
                    .map(|f| f.should_index())
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
        let meta_path = Path::new(META_FILE);
        if !dir.exists(meta_path) {
            dir.write(
                meta_path,
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
        let lock = self.dir.try_lock()?;

        let writer = IndexWriter {
            _dir_guard: lock,
            index: self.clone(),
            segment_writer: None,
            meta: self.load_meta()?,
            memory_budget_bytes,
            flush_threshold_bytes,
        };
        Ok(writer)
    }

    pub fn reader(&self, index_meta: &IndexMeta) -> Result<IndexReader> {
        IndexReader::open(index_meta, self.schema.clone(), self.dir.clone())
    }

    pub fn load_meta(&self) -> Result<IndexMeta> {
        let file = File::open(self.dir.path().join(Path::new(META_FILE)))?;
        let reader = BufReader::new(file);
        let bytes = serde_json::from_reader(reader).unwrap_or(IndexMeta::default());

        Ok(bytes)
    }
}

pub struct IndexWriter {
    _dir_guard: FileLockGuard,
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

        let writer = self
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
        self.persist_meta(&self.meta)?;
        self.merge()?;
        Ok(())
    }

    fn persist_meta(&self, meta: &IndexMeta) -> Result<()> {
        let dir_path = self.index.dir.path().to_path_buf();
        let full = dir_path.join(Path::new(META_FILE));

        let f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&full)?;

        serde_json::to_writer_pretty(f, &meta).unwrap();
        Ok(())
    }

    pub fn merge(&mut self) -> Result<()> {
        let index_reader = IndexReader::open(
            &self.meta,
            self.index.schema.clone(),
            self.index.dir.clone(),
        )?;
        let (segment_meta, consumed_segment_ids) =
            merge_segments(index_reader.segments(), self.index.dir.clone())?;
        self.merge_commit(segment_meta, &consumed_segment_ids)?;
        Ok(())
    }

    fn merge_commit(&mut self, segment_meta: SegmentMeta, consumed: &[SegmentId]) -> Result<()> {
        let mut new_meta = self.meta.clone();
        new_meta.segments = new_meta
            .segments
            .into_iter()
            .filter(|seg| !consumed.contains(&seg.segment_id))
            .collect::<Vec<_>>();
        new_meta.segments.push(segment_meta);
        new_meta.next_segment_id += 1;
        self.persist_meta(&new_meta)?;
        self.meta = new_meta;
        // TODO: best effort garbage collect deleted segments here
        Ok(())
    }
}

#[derive(Debug)]
struct HeapItem<'a> {
    pub term: &'a Term,
    pub entry: &'a TermEntry,
    pub iter: Iter<'a, (Term, TermEntry)>,
    pub segment_index: usize,
}

impl<'a> PartialEq for HeapItem<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.term == other.term
    }
}

impl<'a> Eq for HeapItem<'a> {}

impl<'a> PartialOrd for HeapItem<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<'a> Ord for HeapItem<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.term.cmp(self.term)
    }
}

fn write_doc_posting_bytes_to_segment(bytes: Vec<u8>, dir: Arc<dyn Directory>) {}

struct MergeContext<'a> {
    res: Vec<(&'a Term, &'a TermEntry, usize)>,
    running_offset: &'a mut usize,
    segments: &'a [Segment],
    seg_deltas: &'a [u32],
    merged_term_infos: &'a mut Vec<(Term, TermEntry)>,
    dir: Arc<dyn Directory>,
    post_path: &'a Path,
}

fn flush_group(ctx: MergeContext) -> Result<()> {
    // let mut doc_freq = 0;
    let group_bytes: Vec<u8> = Vec::new();
    let mut new_term_entry = TermEntry {
        len: 0,
        doc_freq: 0,
        offset: *ctx.running_offset,
    };
    let new_term = ctx.res.first().unwrap().0.clone();

    for (term, entry, index) in &ctx.res {
        new_term_entry.doc_freq += entry.doc_freq;
        if let Some(postings) = ctx.segments[*index].postings(term)? {
            // The doc ids are currently localized, we need to remap/merge them so they
            // are global in the final .term file
            let remapped = postings
                .postings
                .iter()
                .map(|dp| DocPosting {
                    doc_id: dp.doc_id + ctx.seg_deltas[*index],
                    term_freq: dp.term_freq,
                    positions: dp.positions.clone(),
                })
                .collect::<Vec<_>>();
            let encoded = wincode::serialize(&remapped).unwrap();
            new_term_entry.len += encoded.len();
            // write new posting to disk (we also need to create a fresh final .post file)
            ctx.dir.append(ctx.post_path, &encoded)?;
        }
    }
    *ctx.running_offset += new_term_entry.len;
    ctx.merged_term_infos.push((new_term, new_term_entry));

    Ok(())
}

fn merge_segments(
    readers: &[SegmentReader],
    dir: Arc<dyn Directory>,
) -> Result<(SegmentMeta, Vec<SegmentId>)> {
    let mut base_doc_id = 0;
    let mut heap: BinaryHeap<HeapItem> = BinaryHeap::new();
    let segments = readers
        .iter()
        .map(|reader| reader.open().unwrap())
        .collect::<Vec<_>>();
    let term_infos = segments
        .iter()
        .map(|seg| seg.term_infos())
        .collect::<Vec<_>>();

    let seg_deltas = segments
        .iter()
        .map(|seg| {
            let id = base_doc_id;
            base_doc_id += seg.max_id();
            id
        })
        .collect::<Vec<_>>();

    let mut res: Vec<(&Term, &TermEntry, usize)> = Vec::new();

    for (i, term_list) in term_infos.iter().enumerate() {
        let mut iter = term_list.iter();
        let (term, entry) = iter.next().unwrap();
        heap.push(HeapItem {
            term,
            entry,
            iter,
            segment_index: i,
        });
    }
    let mut running_offset = 0;
    let merged_segment_id = SegmentId::generate();
    let merged_segment_id_str = merged_segment_id.to_string();
    let mut merged_term_infos: Vec<(Term, TermEntry)> = Vec::new();
    let post_path_buf = format!("{}.post", merged_segment_id_str);
    let term_path_buf = format!("{}.term", merged_segment_id_str);
    let post_path = Path::new(&post_path_buf);
    let term_path = Path::new(&term_path_buf);

    while let Some(HeapItem {
        term,
        entry,
        mut iter,
        segment_index,
    }) = heap.pop()
    {
        if res.is_empty() {
            res.push((term, entry, segment_index));
            let (term, entry) = iter.next().unwrap();
            heap.push(HeapItem {
                term,
                entry,
                iter,
                segment_index,
            });
            continue;
        }

        if let Some((current, _, _)) = res.last()
            && *current == term
        {
            res.push((term, entry, segment_index));
        } else {
            // process and write accumulated terms
            let term_buf = std::mem::take(&mut res);
            let ctx = MergeContext {
                res: term_buf,
                running_offset: &mut running_offset,
                segments: &segments,
                seg_deltas: &seg_deltas,
                merged_term_infos: &mut merged_term_infos,
                dir: dir.clone(),
                post_path,
            };
            flush_group(ctx)?;
            // process and start next term
            res.push((term, entry, segment_index));
        }
        if let Some((next_term, next_entry)) = iter.next() {
            heap.push(HeapItem {
                term: next_term,
                entry: next_entry,
                iter,
                segment_index,
            });
        }
    }
    // If there are still some postings left, flush them.
    if !res.is_empty() {
        let ctx = MergeContext {
            res,
            running_offset: &mut running_offset,
            segments: &segments,
            seg_deltas: &seg_deltas,
            merged_term_infos: &mut merged_term_infos,
            dir: dir.clone(),
            post_path,
        };
        flush_group(ctx)?;
    }

    // write buffered terms to disk now
    dir.write(term_path, &wincode::serialize(&merged_term_infos).unwrap())?;

    // after flushing merged to disk, we need to mark segments for deletion
    let consumed = segments
        .iter()
        .map(|seg| seg.segment_id().clone())
        .collect::<Vec<_>>();

    let segment_meta = SegmentMeta {
        segment_id: merged_segment_id,
        max_doc: base_doc_id,
    };
    Ok((segment_meta, consumed))
}
