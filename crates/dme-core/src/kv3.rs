use crate::error::{Error, Result};

const WHAT: &str = "a compiled file of sound events";

const MAGIC: &[u8; 4] = b"\x053VK";
const TRAILER: u32 = 0xFFEE_DD00;
const HEADER: usize = 20 + 25 * 4;
const UNPACKED: u32 = 0;
const LZ4: u32 = 1;

pub const ZERO: u8 = kind::INT_ZERO;

mod kind {
    pub const NULL: u8 = 1;
    pub const BOOL: u8 = 2;
    pub const INT64: u8 = 3;
    pub const UINT64: u8 = 4;
    pub const DOUBLE: u8 = 5;
    pub const STRING: u8 = 6;
    pub const ARRAY: u8 = 8;
    pub const OBJECT: u8 = 9;
    pub const INT32: u8 = 11;
    pub const UINT32: u8 = 12;
    pub const TRUE: u8 = 13;
    pub const FALSE: u8 = 14;
    pub const INT_ZERO: u8 = 15;
    pub const INT_ONE: u8 = 16;
    pub const DOUBLE_ZERO: u8 = 17;
    pub const DOUBLE_ONE: u8 = 18;
    pub const SHORT_ARRAY: u8 = 24;
    pub const SIDE_ARRAY: u8 = 25;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Plain(u8),
    Bool(bool),
    Int(u8, i64),
    Double(f64),
    String(Option<String>),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
    Typed(u8, u8, Vec<Value>),
    Marked(u8, Box<Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(members) => members.iter().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn for_each_string<'a>(&'a self, visit: &mut impl FnMut(Option<&'a str>, &'a str)) {
        self.strings_under(None, visit);
    }

    fn strings_under<'a>(&'a self, key: Option<&'a str>, visit: &mut impl FnMut(Option<&'a str>, &'a str)) {
        match self {
            Value::String(Some(text)) => visit(key, text),
            Value::Array(items) | Value::Typed(_, _, items) => items.iter().for_each(|item| item.strings_under(key, visit)),
            Value::Marked(_, value) => value.strings_under(key, visit),
            Value::Object(members) => members.iter().for_each(|(name, value)| value.strings_under(Some(name), visit)),
            _ => {}
        }
    }
}

pub struct Compiled<'a> {
    file: &'a [u8],
    entry: usize,
    data: std::ops::Range<usize>,
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| Error::format(WHAT, "the file is cut short"))
}

impl<'a> Compiled<'a> {
    pub fn parse(file: &'a [u8]) -> Result<Compiled<'a>> {
        let blocks_at = 8 + u32_at(file, 8)? as usize;
        let count = u32_at(file, 12)? as usize;
        let mut found = None;
        let mut last_end = 0;
        for index in 0..count.min(64) {
            let entry = blocks_at + 12 * index;
            let start = entry + 4 + u32_at(file, entry + 4)? as usize;
            let end = start + u32_at(file, entry + 8)? as usize;
            if file.get(entry..entry + 4) == Some(b"DATA") {
                found = Some((entry, start..end));
            }
            last_end = last_end.max(end);
        }
        let (entry, data) = found.ok_or_else(|| Error::format(WHAT, "it has no block DATA"))?;
        if data.end != last_end || data.end > file.len() {
            return Err(Error::format(WHAT, "its block DATA is not the last thing in it"));
        }
        Ok(Compiled { file, entry, data })
    }

    pub fn values(&self) -> Result<Value> {
        read(self.data())
    }

    pub fn data(&self) -> &'a [u8] {
        &self.file[self.data.clone()]
    }

    pub fn with_values(&self, value: &Value) -> Result<Vec<u8>> {
        self.with_data(&write(value, self.data())?)
    }

    pub fn with_data(&self, block: &[u8]) -> Result<Vec<u8>> {
        let mut out = self.file[..self.data.start].to_vec();
        out.extend_from_slice(block);
        let total = u32::try_from(out.len()).map_err(|_| Error::format(WHAT, "the file would be too big"))?;
        out[0..4].copy_from_slice(&total.to_le_bytes());
        out[self.entry + 8..self.entry + 12].copy_from_slice(&(block.len() as u32).to_le_bytes());
        Ok(out)
    }
}

#[derive(Default)]
struct Header {
    numbers: [u32; 25],
}

impl Header {
    const COMPRESSION: usize = 0;
    const STRING_BYTES: usize = 2;
    const FIRST_INTS: usize = 3;
    const FIRST_EIGHTS: usize = 4;
    const KINDS: usize = 5;
    const ROOM: usize = 6;
    const TOTAL: usize = 7;
    const TOTAL_PACKED: usize = 8;
    const FIRST_SIZE: usize = 13;
    const FIRST_PACKED: usize = 14;
    const SECOND_SIZE: usize = 15;
    const SECOND_PACKED: usize = 16;
    const BYTES: usize = 17;
    const SHORTS: usize = 18;
    const INTS: usize = 19;
    const EIGHTS: usize = 20;
    const VALUES: usize = 21;
    const OBJECTS: usize = 22;
    const SHORT_ARRAYS: usize = 23;
    const SHORT_ARRAY_ITEMS: usize = 24;

    fn get(&self, field: usize) -> usize {
        self.numbers[field] as usize
    }
}

fn align(at: usize, to: usize) -> usize {
    at.div_ceil(to) * to
}

pub fn read(block: &[u8]) -> Result<Value> {
    let (header, first, second) = unpack(block)?;
    let mut reader = Reader::new(&header, &first, &second)?;
    let value = reader.value(None)?;
    reader.finish()?;
    Ok(value)
}

pub fn unpacked(block: &[u8]) -> Result<([u32; 25], Vec<u8>, Vec<u8>)> {
    let (header, first, second) = unpack(block)?;
    Ok((header.numbers, first, second))
}

fn unpack(block: &[u8]) -> Result<(Header, Vec<u8>, Vec<u8>)> {
    if block.get(..4) != Some(MAGIC.as_slice()) {
        return Err(Error::format(WHAT, "its values are not KeyValues 3 of the fifth version"));
    }
    let mut header = Header::default();
    for (index, number) in header.numbers.iter_mut().enumerate() {
        *number = u32_at(block, 20 + 4 * index)?;
    }
    let cut_short = || Error::format(WHAT, "its values are cut short");
    let (first, second) = match header.numbers[Header::COMPRESSION] {
        UNPACKED => {
            if header.get(Header::FIRST_PACKED) != 0 || header.get(Header::SECOND_PACKED) != 0 {
                return Err(Error::format(WHAT, "its values are not the size it says"));
            }
            let first_end = HEADER.checked_add(header.get(Header::FIRST_SIZE)).ok_or_else(cut_short)?;
            let second_end = first_end.checked_add(header.get(Header::SECOND_SIZE)).ok_or_else(cut_short)?;
            let first = block.get(HEADER..first_end).ok_or_else(cut_short)?.to_vec();
            let second = block.get(first_end..second_end).ok_or_else(cut_short)?.to_vec();
            (first, second)
        }
        LZ4 => {
            let first_end = HEADER.checked_add(header.get(Header::FIRST_PACKED)).ok_or_else(cut_short)?;
            let second_end = first_end.checked_add(header.get(Header::SECOND_PACKED)).ok_or_else(cut_short)?;
            let first = block.get(HEADER..first_end).ok_or_else(cut_short)?;
            let second = block.get(first_end..second_end).ok_or_else(cut_short)?;
            (lz4::unpack(first, header.get(Header::FIRST_SIZE))?, lz4::unpack(second, header.get(Header::SECOND_SIZE))?)
        }
        _ => return Err(Error::format(WHAT, "its values are compressed in a way this tool does not know")),
    };
    Ok((header, first, second))
}

struct Reader<'b> {
    strings: Vec<String>,
    counts: &'b [u8],
    bytes: &'b [u8],
    ints: &'b [u8],
    eights: &'b [u8],
    side: &'b [u8],
    kinds: &'b [u8],
    at: [usize; 6],
}

const COUNTS: usize = 0;
const BYTES: usize = 1;
const INTS: usize = 2;
const EIGHTS: usize = 3;
const SIDE: usize = 4;
const KINDS: usize = 5;

impl<'b> Reader<'b> {
    fn new(header: &Header, first: &'b [u8], second: &'b [u8]) -> Result<Reader<'b>> {
        let cut_short = || Error::format(WHAT, "its values are cut short");
        let take = |buffer: &'b [u8], at: usize, len: usize| at.checked_add(len).and_then(|end| buffer.get(at..end)).ok_or_else(cut_short);
        let string_bytes = header.get(Header::STRING_BYTES);
        let blob = take(first, 0, string_bytes)?;
        let ints_at = align(string_bytes, 4);
        if header.get(Header::FIRST_INTS) != 1 {
            return Err(Error::format(WHAT, "its first buffer is laid out in a way this tool does not know"));
        }
        let count = u32_at(first, ints_at)? as usize;
        let mut strings = Vec::with_capacity(count.min(blob.len()));
        let mut rest = blob;
        for _ in 0..count {
            let end = rest.iter().position(|&byte| byte == 0).ok_or_else(cut_short)?;
            strings.push(String::from_utf8(rest[..end].to_vec()).map_err(|_| Error::format(WHAT, "a string is not UTF-8"))?);
            rest = &rest[end + 1..];
        }
        let side_count = header.get(Header::FIRST_EIGHTS);
        let side_at = if side_count > 0 { align(ints_at + 4, 8) } else { ints_at + 4 };
        let side = take(first, side_at, 8 * side_count)?;
        if side_at + 8 * side_count != first.len() {
            return Err(Error::format(WHAT, "its first buffer is not the size it says"));
        }

        let mut at = 0;
        let counts = take(second, at, 4 * header.get(Header::OBJECTS))?;
        at += counts.len();
        let bytes = take(second, at, header.get(Header::BYTES))?;
        at = align(at + bytes.len(), 2);
        if header.get(Header::SHORTS) != 0 {
            return Err(Error::format(WHAT, "it has two-byte numbers, which this tool does not know"));
        }
        at = align(at, 4);
        let ints = take(second, at, 4 * header.get(Header::INTS))?;
        at = align(at + ints.len(), 8);
        let eights = take(second, at, 8 * header.get(Header::EIGHTS))?;
        at += eights.len();
        let kinds = take(second, at, header.get(Header::KINDS))?;
        at += kinds.len();
        if u32_at(second, at)? != TRAILER || at + 4 != second.len() {
            return Err(Error::format(WHAT, "its second buffer does not end where it should"));
        }
        Ok(Reader { strings, counts, bytes, ints, eights, side, kinds, at: [0; 6] })
    }

    fn next(&mut self, stream: usize, len: usize) -> Result<&'b [u8]> {
        let buffer = match stream {
            COUNTS => self.counts,
            BYTES => self.bytes,
            INTS => self.ints,
            EIGHTS => self.eights,
            SIDE => self.side,
            _ => self.kinds,
        };
        let at = self.at[stream];
        let end = at.checked_add(len).ok_or_else(|| Error::format(WHAT, "its values are cut short"))?;
        let found = buffer.get(at..end).ok_or_else(|| Error::format(WHAT, "its values are cut short"))?;
        self.at[stream] = end;
        Ok(found)
    }

    fn int(&mut self) -> Result<i32> {
        let b = self.next(INTS, 4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn eight(&mut self, stream: usize) -> Result<[u8; 8]> {
        let b = self.next(stream, 8)?;
        Ok([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
    }

    fn string(&mut self, index: i32) -> Result<Option<String>> {
        if index == -1 {
            return Ok(None);
        }
        let found = usize::try_from(index).ok().and_then(|index| self.strings.get(index));
        found.cloned().map(Some).ok_or_else(|| Error::format(WHAT, "a string is not in its list of strings"))
    }

    fn value(&mut self, of: Option<u8>) -> Result<Value> {
        let of = match of {
            Some(of) => of,
            None => self.next(KINDS, 1)?[0],
        };
        let value = match of {
            kind::NULL | kind::TRUE | kind::FALSE | kind::INT_ZERO | kind::INT_ONE | kind::DOUBLE_ZERO | kind::DOUBLE_ONE => Value::Plain(of),
            kind::BOOL => Value::Bool(self.next(BYTES, 1)?[0] != 0),
            kind::INT64 => Value::Int(of, i64::from_le_bytes(self.eight(EIGHTS)?)),
            kind::UINT64 => Value::Int(of, u64::from_le_bytes(self.eight(EIGHTS)?) as i64),
            kind::INT32 => Value::Int(of, i64::from(self.int()?)),
            kind::UINT32 => Value::Int(of, i64::from(self.int()? as u32)),
            kind::DOUBLE => Value::Double(f64::from_le_bytes(self.eight(EIGHTS)?)),
            kind::STRING => {
                let index = self.int()?;
                Value::String(self.string(index)?)
            }
            kind::ARRAY => {
                let len = usize::try_from(self.int()?).map_err(|_| Error::format(WHAT, "an array has a length below zero"))?;
                Value::Array((0..len).map(|_| self.value(None)).collect::<Result<_>>()?)
            }
            kind::OBJECT => {
                let b = self.next(COUNTS, 4)?;
                let len = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
                let mut members = Vec::with_capacity(len.min(4096));
                for _ in 0..len {
                    let index = self.int()?;
                    let name = self.string(index)?.ok_or_else(|| Error::format(WHAT, "a member has no name"))?;
                    members.push((name, self.value(None)?));
                }
                Value::Object(members)
            }
            kind::SHORT_ARRAY | kind::SIDE_ARRAY => {
                let len = usize::from(self.next(BYTES, 1)?[0]);
                let items = self.next(KINDS, 1)?[0];
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    values.push(if of == kind::SIDE_ARRAY {
                        if items != kind::DOUBLE {
                            return Err(Error::format(WHAT, format!("an array in the first buffer holds values of kind {items}")));
                        }
                        Value::Double(f64::from_le_bytes(self.eight(SIDE)?))
                    } else {
                        self.value(Some(items))?
                    });
                }
                Value::Typed(of, items, values)
            }
            other => return Err(Error::format(WHAT, format!("it has a value of kind {other}, which this tool does not know"))),
        };
        Ok(value)
    }

    fn finish(&self) -> Result<()> {
        let lens = [self.counts.len(), self.bytes.len(), self.ints.len(), self.eights.len(), self.side.len(), self.kinds.len()];
        if self.at != lens {
            return Err(Error::format(WHAT, "it holds more than its values"));
        }
        Ok(())
    }
}

#[derive(Default)]
struct Writer {
    strings: Vec<String>,
    index: std::collections::HashMap<String, i32>,
    counts: Vec<u8>,
    bytes: Vec<u8>,
    ints: Vec<u8>,
    eights: Vec<u8>,
    side: Vec<u8>,
    kinds: Vec<u8>,
    item_kinds: usize,
    objects: usize,
    byte_arrays: usize,
    short_arrays: usize,
    short_array_items: usize,
}

impl Writer {
    fn string(&mut self, text: &str) -> i32 {
        if let Some(&index) = self.index.get(text) {
            return index;
        }
        let index = self.strings.len() as i32;
        self.strings.push(text.to_string());
        self.index.insert(text.to_string(), index);
        index
    }

    fn int(&mut self, value: i32) {
        self.ints.extend_from_slice(&value.to_le_bytes());
    }

    fn len_byte(&mut self, len: usize) -> Result<()> {
        let len = u8::try_from(len).map_err(|_| Error::format(WHAT, "a short array would have more than 255 items"))?;
        self.bytes.push(len);
        Ok(())
    }

    fn value(&mut self, value: &Value, of: Option<u8>) -> Result<()> {
        let kind = match value {
            Value::Plain(kind) => *kind,
            Value::Bool(_) => kind::BOOL,
            Value::Int(kind, _) => *kind,
            Value::Double(_) => kind::DOUBLE,
            Value::String(_) => kind::STRING,
            Value::Array(_) => kind::ARRAY,
            Value::Object(_) => kind::OBJECT,
            Value::Typed(kind @ (kind::SHORT_ARRAY | kind::SIDE_ARRAY), _, _) => *kind,
            Value::Typed(..) | Value::Marked(..) => return Err(Error::format(WHAT, "it has a value that the fifth version is not known to write")),
        };
        match of {
            Some(of) if of != kind => return Err(Error::format(WHAT, "an item of a typed array is not of the array's kind")),
            Some(_) => {}
            None => self.kinds.push(kind),
        }
        match value {
            Value::Plain(_) | Value::Marked(..) => {}
            Value::Bool(value) => self.bytes.push(u8::from(*value)),
            Value::Int(kind::INT32 | kind::UINT32, value) => self.int(*value as i32),
            Value::Int(_, value) => self.eights.extend_from_slice(&value.to_le_bytes()),
            Value::Double(value) => self.eights.extend_from_slice(&value.to_le_bytes()),
            Value::String(text) => {
                let index = text.as_deref().map_or(-1, |text| self.string(text));
                self.int(index);
            }
            Value::Array(items) => {
                self.int(items.len() as i32);
                for item in items {
                    self.value(item, None)?;
                }
            }
            Value::Object(members) => {
                self.objects += 1;
                self.counts.extend_from_slice(&(members.len() as u32).to_le_bytes());
                for (name, value) in members {
                    let index = self.string(name);
                    self.int(index);
                    self.value(value, None)?;
                }
            }
            Value::Typed(kind, items_kind, items) => {
                self.len_byte(items.len())?;
                self.byte_arrays += 1;
                if *kind == kind::SHORT_ARRAY {
                    self.short_arrays += 1;
                    self.short_array_items += items.len();
                }
                self.kinds.push(*items_kind);
                self.item_kinds += 1;
                for item in items {
                    if *kind == kind::SIDE_ARRAY {
                        let Value::Double(number) = item else {
                            return Err(Error::format(WHAT, "an array in the first buffer holds something other than doubles"));
                        };
                        self.side.extend_from_slice(&number.to_le_bytes());
                    } else {
                        self.value(item, Some(*items_kind))?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn write(value: &Value, like: &[u8]) -> Result<Vec<u8>> {
    if like.get(..4) != Some(MAGIC.as_slice()) || like.len() < HEADER {
        return Err(Error::format(WHAT, "its values are not KeyValues 3 of the fifth version"));
    }
    if !matches!(u32_at(like, 20)?, UNPACKED | LZ4) {
        return Err(Error::format(WHAT, "its values are compressed in a way this tool does not know"));
    }
    let mut writer = Writer::default();
    writer.value(value, None)?;

    let mut first = Vec::new();
    for text in &writer.strings {
        first.extend_from_slice(text.as_bytes());
        first.push(0);
    }
    let string_bytes = first.len();
    first.resize(align(first.len(), 4), 0);
    first.extend_from_slice(&(writer.strings.len() as u32).to_le_bytes());
    if !writer.side.is_empty() {
        first.resize(align(first.len(), 8), 0);
        first.extend_from_slice(&writer.side);
    }

    let mut second = writer.counts.clone();
    second.extend_from_slice(&writer.bytes);
    second.resize(align(second.len(), 4), 0);
    second.extend_from_slice(&writer.ints);
    second.resize(align(second.len(), 8), 0);
    second.extend_from_slice(&writer.eights);
    second.extend_from_slice(&writer.kinds);
    second.extend_from_slice(&TRAILER.to_le_bytes());

    let room = |count: usize| u16::try_from(count).map_err(|_| Error::format(WHAT, "it would have too many objects or arrays"));
    let mut header = Header::default();
    header.numbers[Header::COMPRESSION] = UNPACKED;
    header.numbers[1] = 0;
    let total = first.len() + second.len();
    let numbers = [
        (Header::STRING_BYTES, string_bytes),
        (Header::FIRST_INTS, 1),
        (Header::FIRST_EIGHTS, writer.side.len() / 8),
        (Header::KINDS, writer.kinds.len()),
        (Header::ROOM, usize::from(room(writer.objects)?) | usize::from(room(writer.byte_arrays)?) << 16),
        (Header::TOTAL, total),
        (Header::TOTAL_PACKED, total),
        (Header::FIRST_SIZE, first.len()),
        (Header::FIRST_PACKED, 0),
        (Header::SECOND_SIZE, second.len()),
        (Header::SECOND_PACKED, 0),
        (Header::BYTES, writer.bytes.len()),
        (Header::SHORTS, 0),
        (Header::INTS, writer.ints.len() / 4),
        (Header::EIGHTS, writer.eights.len() / 8),
        (Header::VALUES, writer.kinds.len() - writer.item_kinds),
        (Header::OBJECTS, writer.objects),
        (Header::SHORT_ARRAYS, writer.short_arrays),
        (Header::SHORT_ARRAY_ITEMS, writer.short_array_items),
    ];
    for (field, number) in numbers {
        header.numbers[field] = u32::try_from(number).map_err(|_| Error::format(WHAT, "it would be too big"))?;
    }

    let mut out = like[..20].to_vec();
    for number in header.numbers {
        out.extend_from_slice(&number.to_le_bytes());
    }
    out.extend_from_slice(&first);
    out.extend_from_slice(&second);
    Ok(out)
}

pub mod lz4 {
    use crate::error::{Error, Result};

    const WHAT: &str = "LZ4 data";

    pub fn unpack(src: &[u8], size: usize) -> Result<Vec<u8>> {
        let wrong = |detail: &str| Error::format(WHAT, detail);
        let mut out = Vec::with_capacity(size.min(64 * 1024 * 1024));
        let mut at = 0;
        let length = |at: &mut usize, mut len: usize| -> Result<usize> {
            if len == 15 {
                loop {
                    let byte = *src.get(*at).ok_or_else(|| wrong("it is cut short"))?;
                    *at += 1;
                    len = len.checked_add(usize::from(byte)).ok_or_else(|| wrong("it is bigger than it says"))?;
                    if byte != 255 {
                        break;
                    }
                }
            }
            Ok(len)
        };
        while at < src.len() {
            let token = src[at];
            at += 1;
            let literals = length(&mut at, usize::from(token >> 4))?;
            let end = at.checked_add(literals).ok_or_else(|| wrong("it is cut short"))?;
            let slice = src.get(at..end).ok_or_else(|| wrong("it is cut short"))?;
            if out.len().checked_add(literals).is_none_or(|total| total > size) {
                return Err(wrong("it is bigger than it says"));
            }
            out.extend_from_slice(slice);
            at = end;
            if at == src.len() {
                break;
            }
            let offset = usize::from(*src.get(at).ok_or_else(|| wrong("it is cut short"))?) | usize::from(*src.get(at + 1).ok_or_else(|| wrong("it is cut short"))?) << 8;
            at += 2;
            let len = length(&mut at, usize::from(token & 15))?.checked_add(4).ok_or_else(|| wrong("it is bigger than it says"))?;
            if offset == 0 || offset > out.len() {
                return Err(wrong("it refers to bytes before its start"));
            }
            if out.len().checked_add(len).is_none_or(|total| total > size) {
                return Err(wrong("it is bigger than it says"));
            }
            let from = out.len() - offset;
            for index in 0..len {
                out.push(out[from + index]);
            }
        }
        if out.len() != size {
            return Err(wrong("it is not the size it says"));
        }
        Ok(out)
    }

    pub fn pack(src: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(src.len() + src.len() / 255 + 16);
        if src.len() < 15 {
            out.push((src.len() as u8) << 4);
        } else {
            out.push(0xF0);
            let mut rest = src.len() - 15;
            while rest >= 255 {
                out.push(255);
                rest -= 255;
            }
            out.push(rest as u8);
        }
        out.extend_from_slice(src);
        out
    }
}

pub mod earlier;

#[cfg(test)]
pub(crate) mod tests;
