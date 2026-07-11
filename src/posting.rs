use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use crate::DocumentId;
use crate::field::FieldId;

#[derive(Debug, Clone, PartialOrd, PartialEq, Ord, Eq, Serialize, Deserialize)]
pub struct Term {
    pub(crate) text: String,
    pub(crate) field: FieldId
}

#[derive(Debug, Clone, Default)]
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