use serde::{Deserialize, Serialize};
use std::ops::Add;
use wincode::{SchemaRead, SchemaWrite};

const PAGE_SIZE: usize = 1 << 20;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    SchemaRead,
    SchemaWrite,
)]
pub struct Addr(u32);

impl Addr {
    pub fn new(page_id: usize, page_offset: usize) -> Self {
        assert!(page_offset < PAGE_SIZE);
        Self((page_id * PAGE_SIZE + page_offset) as u32)
    }

    pub fn page_id(&self) -> usize {
        self.0 as usize / PAGE_SIZE
    }

    pub fn page_offset(&self) -> usize {
        self.0 as usize % PAGE_SIZE
    }
}

impl Add<usize> for Addr {
    type Output = Addr;
    fn add(self, other: usize) -> Self::Output {
        Self((self.0 as usize + other) as u32)
    }
}

impl Add<u32> for Addr {
    type Output = Addr;
    fn add(self, other: u32) -> Self::Output {
        Self(self.0 + other)
    }
}

#[derive(Debug, Clone)]
pub struct Page {
    data: Box<[u8; PAGE_SIZE]>,
    len: usize,
}

impl Page {
    pub fn new() -> Self {
        Self {
            data: Box::new([0; PAGE_SIZE]),
            len: 0,
        }
    }

    pub fn remaining(&self) -> usize {
        PAGE_SIZE - self.len
    }
}

pub struct MemoryArena {
    pages: Vec<Page>,
    bytes_used: usize,
}

impl MemoryArena {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            bytes_used: 0,
        }
    }

    pub fn memory_usage(&self) -> usize {
        self.bytes_used
    }

    pub fn allocate(&mut self, num_bytes: usize) -> Addr {
        assert!(
            num_bytes <= PAGE_SIZE,
            "Arena allocation cannot be larger than {PAGE_SIZE}. Got {num_bytes}."
        );

        if self.pages.last().is_none_or(|p| p.remaining() < num_bytes) {
            self.pages.push(Page::new());
        }

        let page_id = self.pages.len() - 1;
        let page = &mut self.pages[page_id];
        let offset = page.len;
        page.len += num_bytes;
        self.bytes_used += num_bytes;

        Addr::new(page_id, offset)
    }

    pub fn write_bytes(&mut self, addr: Addr, bytes: &[u8]) {
        let page = &mut self.pages[addr.page_id()];
        let start = addr.page_offset();
        page.data[start..start + bytes.len()].copy_from_slice(bytes);
    }

    pub fn read_bytes(&self, addr: Addr, len: usize) -> &[u8] {
        let page = &self.pages[addr.page_id()];
        let start = addr.page_offset();
        &page.data[start..start + len]
    }

    pub fn write_u32(&mut self, addr: Addr, index: usize, value: u32) {
        // addr is always the starting point of the vec, calculate the real position
        // to write by adding to index * 4(bytes)
        self.write_bytes(addr + index * 4, &value.to_le_bytes())
    }

    pub fn read_u32(&self, addr: Addr, index: usize) -> u32 {
        u32::from_le_bytes(self.read_bytes(addr + index * 4, 4).try_into().unwrap())
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, SchemaRead, SchemaWrite)]
pub struct ArenaVec32 {
    addr: Option<Addr>,
    len: usize,
    cap: usize,
}

impl ArenaVec32 {
    pub fn push(&mut self, arena: &mut MemoryArena, value: u32) {
        if self.len == self.cap {
            self.grow(arena)
        }
        arena.write_u32(self.addr.unwrap(), self.len, value);
        self.len += 1;
    }

    pub fn grow(&mut self, arena: &mut MemoryArena) {
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };
        let new_addr = arena.allocate(new_cap);
        if let Some(old_addr) = self.addr {
            for i in 0..self.len {
                arena.write_u32(new_addr, i, arena.read_u32(old_addr, i))
            }
        }

        self.addr = Some(new_addr);
        self.cap = new_cap;
    }

    pub fn to_vec(&self, arena: &MemoryArena) -> Vec<u32> {
        (0..self.len)
            .map(|i| arena.read_u32(self.addr.unwrap(), i))
            .collect()
    }
}
