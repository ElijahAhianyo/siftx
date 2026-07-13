use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use crate::DocumentId;
use crate::field::FieldId;

#[derive(Debug, Clone, PartialOrd, PartialEq, Ord, Eq, Serialize, Deserialize)]
pub struct Term {
    pub(crate) field: FieldId,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermEntry {
    pub offset: usize,
    pub len: usize,
    pub doc_freq: usize
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocPosting {
    doc_id: DocumentId,
    term_freq: u32,
    positions: Vec<u32>
}

#[derive(Debug, Clone, Default)]
pub struct PostingsListBuilder {
   pub(crate) posting:  Vec<DocPosting>
}

pub struct PostingsBuilder {
    map: BTreeMap<Term, PostingsListBuilder>
}

impl PostingsBuilder {
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new()
        }
    }

    pub fn record(&mut self, term: Term, doc_id: DocumentId, position: u32) {
        let entry = self.map.entry(term).or_default();
        match entry.posting.last_mut() {
            Some(doc_posting) if doc_posting.doc_id == doc_id => {
                doc_posting.term_freq += 1;
                doc_posting.positions.push(position);
            }
            _ => {
                entry.posting.push(
                    DocPosting {
                        doc_id,
                        term_freq: 1,
                        positions: vec![position]
                    }
                )
            }
        }

    }
}

pub struct IntoPostingIter{
    inner: std::collections::btree_map::IntoIter<Term, PostingsListBuilder>
}

impl Iterator for IntoPostingIter{
    type Item = (Term, PostingList);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(term, p)| (term, PostingList{postings: p.posting}))
    }
}

impl IntoIterator for PostingsBuilder{
    type Item = (Term, PostingList);
    type IntoIter = IntoPostingIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoPostingIter {
            inner: self.map.into_iter()
        }

    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostingList{
    pub(crate) postings: Vec<DocPosting>
}

impl PostingList {
    pub fn doc_freq(&self) -> usize {
        self.postings.len()
    }
}