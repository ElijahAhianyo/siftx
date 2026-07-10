use std::collections::HashMap;
use std::str::CharIndices;
use crate::token::Token;

pub trait Tokenizer {
    type TokenStream<'a>: TokenStream
    where Self: 'a;
    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a>;
}


#[derive(Clone, Default)]
struct BasicTokenizer{
    token: Token
}

impl Tokenizer for BasicTokenizer {
    type TokenStream<'a> = BasicTokenizerStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        self.token.clear();
        BasicTokenizerStream {
            token: &mut self.token,
            chars: text.char_indices(),
            text
        }
    }
}

struct BasicTokenizerStream<'a>{
    token: &'a mut Token,
    chars: CharIndices<'a>,
    text: &'a str,
}

impl BasicTokenizerStream<'_> {
    fn find_token_end( &mut self) -> usize {
        (&mut self.chars).filter(|(_, c)| !c.is_alphanumeric())
            .map(|(i, _)| i)
            .next().unwrap_or(self.text.len())
    }
}

impl<'a> TokenStream for BasicTokenizerStream<'a> {
    fn advance(&mut self) -> bool {
        self.token.term.clear();
        self.token.position = self.token.position.wrapping_add(1);

        while let Some((i, c)) = self.chars.next(){
            if c.is_alphanumeric(){
                let end_offset = self.find_token_end();
                self.token.term.push_str(&self.text[i..end_offset]);
                self.token.start_offset = i;
                self.token.end_offset = end_offset;
                return true;
            }
        }
        false
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}

pub trait BoxedTokenizer{
    fn box_token_stream<'a>(&'a mut self, text: &'a str) -> BoxedTokenStream<'a>;

    fn box_clone(&self) -> Box<dyn BoxedTokenizer>;
}

impl Tokenizer for Box<dyn BoxedTokenizer>{
    type TokenStream<'a> = BoxedTokenStream<'a>;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        // *self -> Box<dyn BoxedTokenizer>
        // **self -> &dyn BoxedTokenizer (which is the concrete type we want)
        (**self).box_token_stream(text)
    }
}

impl Clone for Box<dyn BoxedTokenizer>{
    fn clone(&self) -> Self {
        (**self).box_clone()
    }
}


impl <T: Tokenizer + Clone + 'static> BoxedTokenizer for T{
    fn box_token_stream<'a>(&'a mut self, text: &'a str) -> BoxedTokenStream<'a> {
        BoxedTokenStream::new(self.token_stream(text))
    }

    fn box_clone(&self) -> Box<dyn BoxedTokenizer> {
        Box::new(self.clone())
    }
}


pub struct BoxedTokenStream<'a>(Box<dyn TokenStream + 'a>);

impl <'a> BoxedTokenStream<'a>{
    pub fn new<T: TokenStream + 'a>(token_stream: T) -> Self{
        Self(Box::new(token_stream))
    }
}

impl<'a> TokenStream for BoxedTokenStream<'a> {
    fn advance(&mut self) -> bool {
        self.0.advance()
    }

    fn token(&self) -> &Token {
        self.0.token()
    }

    fn token_mut(&mut self) -> &mut Token {
        self.0.token_mut()
    }
}

pub trait TokenStream{
    fn advance(&mut self) -> bool;
    fn token(&self) -> &Token;
    fn token_mut(&mut self) -> &mut Token;
    fn next(&mut self) -> Option<&Token>{
        if self.advance(){
            Some(self.token())
        } else {
            None
        }
    }

}

#[derive(Clone)]
pub struct TextAnalyzer{
    tokenizer: Box<dyn BoxedTokenizer>
}

impl TextAnalyzer{
    pub fn new<T: BoxedTokenizer +  'static>(tokenizer: T) -> Self {
        Self{tokenizer: Box::new(tokenizer)}
    }

    pub fn filter<F: TokenFilter + 'static>(&mut self, filter: F) -> &mut Self where <F as TokenFilter>::Tokenizer<Box<dyn BoxedTokenizer>>: Clone{
        self.tokenizer = Box::new(filter.transform(self.tokenizer.clone()));
        self
    }
}


pub trait TokenFilter{
    type Tokenizer<T: Tokenizer>: Tokenizer;

    fn transform<T: Tokenizer>(&self, tokenizer: T)-> Self::Tokenizer<T>;
}

#[derive(Clone)]
pub struct LowerCaseFilter<T>{
    tokenizer: T,
}


impl<T: Tokenizer> Tokenizer for LowerCaseFilter<T>{
    type TokenStream<'a> = LowerCaseStream<T::TokenStream<'a>> where T: 'a;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        LowerCaseStream{
            tail: self.tokenizer.token_stream(text)
        }
    }
}

pub struct LowerCaseStream<T>{
    tail: T
}

impl <T: TokenStream> TokenStream for LowerCaseStream<T>{

    fn advance(&mut self) -> bool {
        if !self.tail.advance() {
            return false;
        }
        self.tail.token_mut().term.make_ascii_lowercase();
        true
    }

    fn token(&self) -> &Token {
        self.tail.token()
    }

    fn token_mut(&mut self) -> &mut Token {
        self.tail.token_mut()
    }

}


struct LowerCaser;


impl TokenFilter for LowerCaser {
    type Tokenizer<T: Tokenizer> = LowerCaseFilter<T>;

    fn transform<T: Tokenizer>(&self, tokenizer: T) -> Self::Tokenizer<T> {
        LowerCaseFilter{
            tokenizer
        }
    }
}


#[derive(Clone)]
pub struct TokenizerManager{
    tokenizers: HashMap<String, TextAnalyzer>
}


impl TokenizerManager{
    pub fn new() -> Self{
        Self{
            tokenizers: HashMap::new()
        }
    }
    
    pub fn register<T: Into<String>>(&mut self, name: T, tokenizer: TextAnalyzer){
        self.tokenizers.insert(name.into(), tokenizer);
    }
}


impl Default for TokenizerManager{
    fn default() -> Self {
        let mut this = Self::new();
        this.register(
            "default",
            TextAnalyzer::new(BasicTokenizer::default())
        );
        this
    }   
}