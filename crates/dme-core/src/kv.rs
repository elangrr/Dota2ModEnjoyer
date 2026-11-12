use std::fmt;

use memchr::{memchr, memchr2, memchr3};

use crate::error::{Error, Result};

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

#[derive(Clone, Copy)]
pub struct Block<'a> {
    src: &'a [u8],
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Pair<'a> {
    pub key: &'a [u8],
    pub value: Value<'a>,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum Value<'a> {
    Str(&'a [u8]),
    Block(Block<'a>),
}

impl<'a> Value<'a> {
    pub fn as_str(self) -> Option<&'a [u8]> {
        match self {
            Value::Str(s) => Some(s),
            Value::Block(_) => None,
        }
    }

    pub fn as_block(self) -> Option<Block<'a>> {
        match self {
            Value::Str(_) => None,
            Value::Block(b) => Some(b),
        }
    }
}

impl<'a> Block<'a> {
    pub fn document(src: &'a [u8]) -> Block<'a> {
        let start = if src.starts_with(BOM) { BOM.len() } else { 0 };
        Block {
            src,
            start,
            end: src.len(),
        }
    }

    pub fn pairs(&self) -> Pairs<'a> {
        Pairs {
            src: self.src,
            pos: self.start,
            end: self.end,
        }
    }

    pub fn get(&self, key: &str) -> Option<Value<'a>> {
        self.pairs()
            .find(|p| p.key.eq_ignore_ascii_case(key.as_bytes()))
            .map(|p| p.value)
    }

    pub fn str(&self, key: &str) -> Option<&'a [u8]> {
        self.get(key).and_then(Value::as_str)
    }

    pub fn block(&self, key: &str) -> Option<Block<'a>> {
        self.get(key).and_then(Value::as_block)
    }

    pub fn for_each_string(&self, visit: &mut impl FnMut(&'a [u8], &'a [u8])) {
        for pair in self.pairs() {
            match pair.value {
                Value::Str(value) => visit(pair.key, value),
                Value::Block(inner) => inner.for_each_string(visit),
            }
        }
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }
}

impl fmt::Debug for Block<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Block({}..{})", self.start, self.end)
    }
}

pub struct Pairs<'a> {
    src: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> Iterator for Pairs<'a> {
    type Item = Pair<'a>;

    fn next(&mut self) -> Option<Pair<'a>> {
        let (src, end) = (self.src, self.end);
        let start = skip_trivia(src, self.pos, end);
        if start >= end {
            return None;
        }
        let (key, after_key) = read_string(src, start, end)?;
        let value_at = skip_trivia(src, after_key, end);
        if value_at >= end {
            return None;
        }
        if src[value_at] == b'{' {
            let close = find_block_end(src, value_at + 1, end)?;
            self.pos = close + 1;
            Some(Pair {
                key,
                value: Value::Block(Block {
                    src,
                    start: value_at + 1,
                    end: close,
                }),
                start,
                end: close + 1,
            })
        } else {
            let (value, after_value) = read_string(src, value_at, end)?;
            self.pos = after_value;
            Some(Pair {
                key,
                value: Value::Str(value),
                start,
                end: after_value,
            })
        }
    }
}

pub fn validate(src: &[u8], what: &str) -> Result<()> {
    let end = src.len();
    let mut pos = if src.starts_with(BOM) { BOM.len() } else { 0 };
    let mut depth = 0usize;
    loop {
        pos = skip_trivia(src, pos, end);
        if pos >= end {
            break;
        }
        match src[pos] {
            b'{' => {
                depth += 1;
                pos += 1;
            }
            b'}' => {
                if depth == 0 {
                    return Err(Error::format(
                        what,
                        format!("closing brace without an opening brace at byte {pos}"),
                    ));
                }
                depth -= 1;
                pos += 1;
            }
            b'"' => {
                pos = skip_quoted(src, pos, end).ok_or_else(|| {
                    Error::format(what, format!("text in quotes never ends, from byte {pos}"))
                })?;
            }
            _ => pos = skip_bare(src, pos, end),
        }
    }
    if depth != 0 {
        return Err(Error::format(
            what,
            format!("{depth} opening brace(s) are never closed"),
        ));
    }
    Ok(())
}

pub fn unescape(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    if !text.contains('\\') {
        return text.into_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn skip_trivia(src: &[u8], mut pos: usize, end: usize) -> usize {
    while pos < end {
        let b = src[pos];
        if b <= b' ' {
            pos += 1;
        } else if b == b'/' && pos + 1 < end && src[pos + 1] == b'/' {
            pos = match memchr(b'\n', &src[pos..end]) {
                Some(i) => pos + i + 1,
                None => end,
            };
        } else {
            break;
        }
    }
    pos
}

fn skip_quoted(src: &[u8], open: usize, end: usize) -> Option<usize> {
    let mut pos = open + 1;
    while pos < end {
        let i = pos + memchr2(b'"', b'\\', &src[pos..end])?;
        if src[i] == b'"' {
            return Some(i + 1);
        }
        pos = i + 2;
    }
    None
}

fn skip_bare(src: &[u8], mut pos: usize, end: usize) -> usize {
    while pos < end && !matches!(src[pos], 0..=b' ' | b'{' | b'}' | b'"') {
        pos += 1;
    }
    pos
}

fn read_string(src: &[u8], pos: usize, end: usize) -> Option<(&[u8], usize)> {
    match src[pos] {
        b'{' | b'}' => None,
        b'"' => {
            let after = skip_quoted(src, pos, end)?;
            Some((&src[pos + 1..after - 1], after))
        }
        _ => {
            let after = skip_bare(src, pos, end);
            Some((&src[pos..after], after))
        }
    }
}

fn find_block_end(src: &[u8], mut pos: usize, end: usize) -> Option<usize> {
    let mut depth = 1usize;
    while pos < end {
        let i = pos + memchr3(b'"', b'{', b'}', &src[pos..end])?;
        if let Some(comment) = find_comment(&src[pos..i]) {
            let line = pos + comment;
            pos = line + memchr(b'\n', &src[line..end])? + 1;
            continue;
        }
        match src[i] {
            b'"' => pos = skip_quoted(src, i, end)?,
            b'{' => {
                depth += 1;
                pos = i + 1;
            }
            _ => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                pos = i + 1;
            }
        }
    }
    None
}

fn find_comment(region: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = memchr(b'/', &region[from..]) {
        let at = from + i;
        if region.get(at + 1) == Some(&b'/') {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(bytes: &[u8]) -> &str {
        std::str::from_utf8(bytes).unwrap()
    }

    const SAMPLE: &str = "\"root\"\r\n{\r\n\t\"a\"\t\t\"1\"\r\n\t\"b\"\r\n\t{\r\n\t\t\"x\"\t\"{not a brace}\"\r\n\t\t// } \" a comment with a brace and a quote\r\n\t\t\"y\"\t\"quote \\\" inside\"\r\n\t}\r\n\tbare\tword // trailing comment\r\n\t\"A\"\t\"second a\"\r\n}\r\n";

    #[test]
    fn reads_pairs_at_one_level_only() {
        let doc = Block::document(SAMPLE.as_bytes());
        let root = doc.block("root").unwrap();
        let keys: Vec<&str> = root.pairs().map(|p| text(p.key)).collect();
        assert_eq!(keys, ["a", "b", "bare", "A"]);

        let b = root.block("b").unwrap();
        let inner: Vec<(&str, &str)> = b
            .pairs()
            .map(|p| (text(p.key), text(p.value.as_str().unwrap())))
            .collect();
        assert_eq!(inner, [("x", "{not a brace}"), ("y", "quote \\\" inside")]);
    }

    #[test]
    fn keys_ignore_case_and_first_match_wins() {
        let root = Block::document(SAMPLE.as_bytes()).block("ROOT").unwrap();
        assert_eq!(text(root.str("A").unwrap()), "1");
        assert_eq!(text(root.str("bare").unwrap()), "word");
        assert!(root.str("b").is_none());
        assert!(root.block("a").is_none());
        assert!(root.get("missing").is_none());
    }

    #[test]
    fn pair_and_block_offsets_point_at_the_source_text() {
        let src = SAMPLE.as_bytes();
        let root = Block::document(src).block("root").unwrap();
        let a = root.pairs().next().unwrap();
        assert_eq!(text(&src[a.start..a.end]), "\"a\"\t\t\"1\"");

        let b = root.pairs().nth(1).unwrap();
        let whole = text(&src[b.start..b.end]);
        assert!(whole.starts_with("\"b\"\r\n\t{"));
        assert!(whole.ends_with('}'));
        let block = b.value.as_block().unwrap();
        assert_eq!(src[block.start() - 1], b'{');
        assert_eq!(src[block.end()], b'}');
    }

    #[test]
    fn visits_strings_at_every_depth() {
        let root = Block::document(SAMPLE.as_bytes()).block("root").unwrap();
        let mut seen = Vec::new();
        root.for_each_string(&mut |key, value| seen.push((text(key), text(value))));
        assert_eq!(
            seen,
            [("a", "1"), ("x", "{not a brace}"), ("y", "quote \\\" inside"), ("bare", "word"), ("A", "second a")]
        );
    }

    #[test]
    fn skips_byte_order_mark() {
        let mut src = BOM.to_vec();
        src.extend_from_slice(b"\"k\" \"v\"");
        assert_eq!(Block::document(&src).str("k").unwrap(), b"v");
        validate(&src, "test").unwrap();
    }

    #[test]
    fn validate_accepts_good_and_rejects_broken_text() {
        validate(SAMPLE.as_bytes(), "test").unwrap();
        validate(b"", "test").unwrap();
        assert!(validate(b"\"a\" { \"b\" \"c\" ", "test").is_err());
        assert!(validate(b"\"a\" { } }", "test").is_err());
        assert!(validate(b"\"a\" \"never ends", "test").is_err());
    }

    #[test]
    fn broken_text_stops_iteration_instead_of_panicking() {
        for src in [
            &b"\"a\" {"[..],
            b"\"a\"",
            b"\"a\" \"b",
            b"{",
            b"}",
            b"\"a\" { \"b\" \"\\",
            b"\"",
        ] {
            let _ = Block::document(src).pairs().count();
        }
    }

    #[test]
    fn unescape_resolves_backslashes() {
        assert_eq!(unescape(b"C:\\\\Program Files\\\\Steam"), "C:\\Program Files\\Steam");
        assert_eq!(unescape(b"say \\\"hi\\\"\\n"), "say \"hi\"\n");
        assert_eq!(unescape(b"plain"), "plain");
    }
}
