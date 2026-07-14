#[derive(Debug, Clone)]
pub struct Token {
    pub term: String,
    pub position: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}

impl Token {
    pub fn clear(&mut self) {
        self.term.clear();
        self.position = usize::MAX;
        self.start_offset = 0;
        self.end_offset = 0;
    }
}

impl Default for Token {
    fn default() -> Self {
        Self {
            term: String::new(),
            position: usize::MAX,
            start_offset: 0,
            end_offset: 0,
        }
    }
}
