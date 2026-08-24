use crate::DocumentId;
use crate::arena::{ArenaVec32, MemoryArena};
use crate::field::FieldId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use wincode::{SchemaRead, SchemaWrite};

#[derive(
    Debug,
    Clone,
    Hash,
    PartialOrd,
    PartialEq,
    Ord,
    Eq,
    Serialize,
    Deserialize,
    SchemaRead,
    SchemaWrite,
)]
pub struct Term {
    pub(crate) field: FieldId,
    pub(crate) text: String,
}

#[derive(
    Debug, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize, SchemaRead, SchemaWrite,
)]
pub struct TermEntry {
    pub offset: usize,
    pub len: usize,
    pub doc_freq: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, SchemaRead, SchemaWrite)]
pub struct DocPostingBuilder {
    doc_id: DocumentId,
    term_freq: u32,
    positions: ArenaVec32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, SchemaRead, SchemaWrite)]
pub struct DocPosting {
    pub doc_id: DocumentId,
    pub term_freq: u32,
    pub positions: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct PostingsListBuilder {
    pub(crate) posting: Vec<DocPostingBuilder>,
}

pub struct PostingsBuilder {
    map: HashMap<Term, PostingsListBuilder>,
    arena: MemoryArena,
    extra_mem_used: usize,
}

impl PostingsBuilder {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            arena: MemoryArena::new(),
            extra_mem_used: 0,
        }
    }

    pub fn record(&mut self, term: Term, doc_id: DocumentId, position: u32) {
        match self.map.entry(term) {
            Entry::Vacant(v) => {
                self.extra_mem_used +=
                    size_of::<Term>() + v.key().text.capacity() + size_of::<PostingsListBuilder>();
                let mut dp = DocPostingBuilder {
                    doc_id,
                    term_freq: 1,
                    positions: ArenaVec32::default(),
                };
                dp.positions.push(&mut self.arena, position);
                self.extra_mem_used += size_of::<DocPostingBuilder>();
                v.insert(PostingsListBuilder { posting: vec![dp] });
            }
            Entry::Occupied(mut o) => {
                let entry = o.get_mut();
                match entry.posting.last_mut() {
                    Some(dp) if dp.doc_id == doc_id => {
                        dp.term_freq += 1;
                        dp.positions.push(&mut self.arena, position);
                    }
                    _ => {
                        let cap_before = entry.posting.capacity();
                        let mut dp = DocPostingBuilder {
                            doc_id,
                            term_freq: 1,
                            positions: ArenaVec32::default(),
                        };
                        dp.positions.push(&mut self.arena, position);
                        entry.posting.push(dp);
                        if entry.posting.capacity() > cap_before {
                            self.extra_mem_used += (entry.posting.capacity() - cap_before)
                                * size_of::<DocPostingBuilder>();
                        }
                    }
                }
            }
        }
    }

    pub fn memory_usage(&self) -> usize {
        self.arena.memory_usage() + self.extra_mem_used
    }
}

pub struct IntoPostingIter {
    inner: std::vec::IntoIter<(Term, PostingsListBuilder)>,
    arena: MemoryArena,
}

impl Iterator for IntoPostingIter {
    type Item = (Term, PostingList);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(term, p)| {
            let postings: Vec<DocPosting> = p
                .posting
                .into_iter()
                .map(|dp| DocPosting {
                    doc_id: dp.doc_id,
                    term_freq: dp.term_freq,
                    positions: dp.positions.to_vec(&mut self.arena),
                })
                .collect();

            (term, PostingList { postings })
        })
    }
}

impl IntoIterator for PostingsBuilder {
    type Item = (Term, PostingList);
    type IntoIter = IntoPostingIter;

    fn into_iter(self) -> Self::IntoIter {
        let mut entries = self.map.into_iter().collect::<Vec<_>>();
        entries.sort_by(|(a, _), (b, _)| a.cmp(b));
        IntoPostingIter {
            inner: entries.into_iter(),
            arena: self.arena,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, SchemaRead, SchemaWrite)]
pub struct PostingList {
    pub(crate) postings: Vec<DocPosting>,
}

impl PostingList {
    pub fn doc_freq(&self) -> usize {
        self.postings.len()
    }
}
