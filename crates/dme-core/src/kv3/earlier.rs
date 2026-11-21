use super::{TRAILER, Value, align, kind, lz4};
use crate::error::{Error, Result};

const WHAT: &str = "a compiled particle file";

const LONG_ARRAY: u8 = 10;
const MARK: u8 = 0x80;

mod at {
    pub const COMPRESSION: usize = 20;
    pub const COMPRESSION_MORE: usize = 24;
    pub const BYTES: usize = 28;
    pub const INTS: usize = 32;
    pub const EIGHTS: usize = 36;
    pub const STRINGS_AND_KINDS: usize = 40;
    pub const OBJECTS: usize = 44;
    pub const ARRAYS: usize = 46;
    pub const SIZE: usize = 48;
    pub const PACKED: usize = 52;
    pub const BLOCKS: usize = 56;
    pub const TWOS: usize = 64;
}

const UNPACKED: u32 = 0;
const LZ4: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    header: Vec<u8>,
    strings: Vec<String>,
    pub root: Value,
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let found = bytes.get(at..at + 4).ok_or_else(|| Error::format(WHAT, "its values are cut short"))?;
    Ok(u32::from_le_bytes([found[0], found[1], found[2], found[3]]))
}

pub fn is_earlier(block: &[u8]) -> bool {
    block.len() >= 4 && (2..=4).contains(&block[0]) && &block[1..4] == b"3VK"
}

fn unpack(block: &[u8]) -> Result<(usize, Vec<u8>)> {
    if !is_earlier(block) {
        return Err(Error::format(WHAT, "its values are not KeyValues 3 of the second, third or fourth version"));
    }
    let header = if block[0] >= 4 { 72 } else { 64 };
    if block.len() < header {
        return Err(Error::format(WHAT, "its values are cut short"));
    }
    if u32_at(block, at::BLOCKS)? != 0 || (block[0] >= 4 && u32_at(block, at::TWOS)? != 0) {
        return Err(Error::format(WHAT, "it holds blocks of bytes or two-byte numbers, which this tool does not know"));
    }
    let size = u32_at(block, at::SIZE)? as usize;
    let packed = block.get(header..).unwrap_or_default();
    let compression = u32_at(block, at::COMPRESSION)?;
    if packed.len() != u32_at(block, at::PACKED)? as usize || (compression == UNPACKED && packed.len() != size) {
        return Err(Error::format(WHAT, "its values are not the size it says"));
    }
    let buffer = match compression {
        UNPACKED => packed.to_vec(),
        LZ4 => lz4::unpack(packed, size)?,
        _ => return Err(Error::format(WHAT, "its values are compressed in a way this tool does not know")),
    };
    Ok((header, buffer))
}

pub fn unpacked(block: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    let (header, buffer) = unpack(block)?;
    let mut header = block[..header].to_vec();
    header[at::PACKED..at::PACKED + 4].fill(0);
    Ok((header, buffer))
}

pub fn read(block: &[u8]) -> Result<Document> {
    let (header, buffer) = unpack(block)?;
    let cut_short = || Error::format(WHAT, "its values are cut short");
    let take = |at: usize, len: usize| buffer.get(at..at + len).ok_or_else(cut_short);
    let bytes = take(0, u32_at(block, at::BYTES)? as usize)?;
    let ints_at = align(bytes.len(), 4);
    let ints = take(ints_at, 4 * u32_at(block, at::INTS)? as usize)?;
    let eights_at = align(ints_at + ints.len(), 8);
    let eights = take(eights_at, 8 * u32_at(block, at::EIGHTS)? as usize)?;
    let rest_at = eights_at + eights.len();
    let rest = take(rest_at, u32_at(block, at::STRINGS_AND_KINDS)? as usize)?;
    if u32_at(&buffer, rest_at + rest.len())? != TRAILER || rest_at + rest.len() + 4 != buffer.len() {
        return Err(Error::format(WHAT, "its values do not end where they should"));
    }

    let mut reader = Reader { strings: Vec::new(), bytes, ints, eights, kinds: &[], at: [0; 4] };
    let count = usize::try_from(reader.int()?).map_err(|_| Error::format(WHAT, "it has fewer than no strings"))?;
    let mut kinds = rest;
    for _ in 0..count {
        let end = kinds.iter().position(|&byte| byte == 0).ok_or_else(cut_short)?;
        reader.strings.push(String::from_utf8(kinds[..end].to_vec()).map_err(|_| Error::format(WHAT, "a string is not UTF-8"))?);
        kinds = &kinds[end + 1..];
    }
    reader.kinds = kinds;
    let root = reader.value(None)?;
    if reader.at != [bytes.len(), ints.len(), eights.len(), kinds.len()] {
        return Err(Error::format(WHAT, "it holds more than its values"));
    }
    Ok(Document { header: block[..header].to_vec(), strings: reader.strings, root })
}

struct Reader<'b> {
    strings: Vec<String>,
    bytes: &'b [u8],
    ints: &'b [u8],
    eights: &'b [u8],
    kinds: &'b [u8],
    at: [usize; 4],
}

const BYTES: usize = 0;
const INTS: usize = 1;
const EIGHTS: usize = 2;
const KINDS: usize = 3;

impl<'b> Reader<'b> {
    fn next(&mut self, stream: usize, len: usize) -> Result<&'b [u8]> {
        let buffer = [self.bytes, self.ints, self.eights, self.kinds][stream];
        let at = self.at[stream];
        let found = buffer.get(at..at + len).ok_or_else(|| Error::format(WHAT, "its values are cut short"))?;
        self.at[stream] += len;
        Ok(found)
    }

    fn int(&mut self) -> Result<i32> {
        let b = self.next(INTS, 4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn len(&mut self) -> Result<usize> {
        usize::try_from(self.int()?).map_err(|_| Error::format(WHAT, "an object or array has a length below zero"))
    }

    fn eight(&mut self) -> Result<[u8; 8]> {
        let b = self.next(EIGHTS, 8)?;
        Ok([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
    }

    fn string(&mut self) -> Result<Option<String>> {
        let index = self.int()?;
        if index == -1 {
            return Ok(None);
        }
        let found = usize::try_from(index).ok().and_then(|index| self.strings.get(index));
        found.cloned().map(Some).ok_or_else(|| Error::format(WHAT, "a string is not in its list of strings"))
    }

    fn value(&mut self, of: Option<u8>) -> Result<Value> {
        let (of, mark) = match of {
            Some(of) => (of, None),
            None => match self.next(KINDS, 1)?[0] {
                marked if marked & MARK != 0 => (marked & !MARK, Some(self.next(KINDS, 1)?[0])),
                plain => (plain, None),
            },
        };
        let value = match of {
            kind::NULL | kind::TRUE | kind::FALSE | kind::INT_ZERO | kind::INT_ONE | kind::DOUBLE_ZERO | kind::DOUBLE_ONE => Value::Plain(of),
            kind::BOOL => match self.next(BYTES, 1)?[0] {
                byte @ (0 | 1) => Value::Bool(byte == 1),
                _ => return Err(Error::format(WHAT, "a yes or no is neither")),
            },
            kind::INT64 => Value::Int(of, i64::from_le_bytes(self.eight()?)),
            kind::UINT64 => Value::Int(of, u64::from_le_bytes(self.eight()?) as i64),
            kind::INT32 => Value::Int(of, i64::from(self.int()?)),
            kind::UINT32 => Value::Int(of, i64::from(self.int()? as u32)),
            kind::DOUBLE => Value::Double(f64::from_le_bytes(self.eight()?)),
            kind::STRING => Value::String(self.string()?),
            kind::ARRAY => {
                let len = self.len()?;
                Value::Array((0..len).map(|_| self.value(None)).collect::<Result<_>>()?)
            }
            kind::OBJECT => {
                let len = self.len()?;
                let mut members = Vec::with_capacity(len.min(4096));
                for _ in 0..len {
                    let name = self.string()?.ok_or_else(|| Error::format(WHAT, "a member has no name"))?;
                    members.push((name, self.value(None)?));
                }
                Value::Object(members)
            }
            LONG_ARRAY | kind::SHORT_ARRAY => {
                let len = if of == LONG_ARRAY { self.len()? } else { usize::from(self.next(BYTES, 1)?[0]) };
                let items = self.next(KINDS, 1)?[0];
                if items & MARK != 0 {
                    return Err(Error::format(WHAT, "an array's items are marked, which this tool does not know"));
                }
                Value::Typed(of, items, (0..len).map(|_| self.value(Some(items))).collect::<Result<_>>()?)
            }
            other => return Err(Error::format(WHAT, format!("it has a value of kind {other}, which this tool does not know"))),
        };
        Ok(match mark {
            Some(mark) => Value::Marked(mark, Box::new(value)),
            None => value,
        })
    }
}

struct Writer {
    strings: Vec<String>,
    index: std::collections::HashMap<String, i32>,
    bytes: Vec<u8>,
    ints: Vec<u8>,
    eights: Vec<u8>,
    kinds: Vec<u8>,
    objects: usize,
    arrays: usize,
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

    fn value(&mut self, value: &Value, of: Option<u8>) -> Result<()> {
        let (value, mark) = match value {
            Value::Marked(mark, inner) => (inner.as_ref(), Some(*mark)),
            plain => (plain, None),
        };
        let kind = match value {
            Value::Plain(kind) => *kind,
            Value::Bool(_) => kind::BOOL,
            Value::Int(kind, _) => *kind,
            Value::Double(_) => kind::DOUBLE,
            Value::String(_) => kind::STRING,
            Value::Array(_) => kind::ARRAY,
            Value::Object(_) => kind::OBJECT,
            Value::Typed(kind @ (LONG_ARRAY | kind::SHORT_ARRAY), _, _) => *kind,
            Value::Typed(..) | Value::Marked(..) => return Err(Error::format(WHAT, "it has a value that these versions are not known to write")),
        };
        match (of, mark) {
            (Some(of), None) if of == kind => {}
            (Some(_), _) => return Err(Error::format(WHAT, "an item of a typed array is marked or not of the array's kind")),
            (None, Some(mark)) => self.kinds.extend_from_slice(&[kind | MARK, mark]),
            (None, None) => self.kinds.push(kind),
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
                self.arrays += 1;
                self.int(items.len() as i32);
                for item in items {
                    self.value(item, None)?;
                }
            }
            Value::Object(members) => {
                self.objects += 1;
                self.int(members.len() as i32);
                for (name, value) in members {
                    let index = self.string(name);
                    self.int(index);
                    self.value(value, None)?;
                }
            }
            Value::Typed(kind, items_kind, items) => {
                self.arrays += 1;
                if *kind == LONG_ARRAY {
                    self.int(items.len() as i32);
                } else {
                    self.bytes.push(u8::try_from(items.len()).map_err(|_| Error::format(WHAT, "a short array would have more than 255 items"))?);
                }
                self.kinds.push(*items_kind);
                for item in items {
                    self.value(item, Some(*items_kind))?;
                }
            }
        }
        Ok(())
    }
}

impl Document {
    #[cfg(test)]
    pub(crate) fn made(version: u8, compression: u32, root: Value) -> Document {
        let mut header = vec![version, b'3', b'V', b'K'];
        header.extend_from_slice(&[7; 16]);
        header.extend_from_slice(&compression.to_le_bytes());
        header.resize(if version >= 4 { 72 } else { 64 }, 0);
        Document { header, strings: Vec::new(), root }
    }

    pub fn not_compressed(&mut self) {
        self.header[at::COMPRESSION..at::COMPRESSION_MORE + 4].fill(0);
    }

    pub fn write(&self) -> Result<Vec<u8>> {
        let index = self.strings.iter().enumerate().rev().map(|(index, text)| (text.clone(), index as i32)).collect();
        let mut writer = Writer { strings: self.strings.clone(), index, bytes: Vec::new(), ints: vec![0; 4], eights: Vec::new(), kinds: Vec::new(), objects: 0, arrays: 0 };
        writer.value(&self.root, None)?;
        let count = writer.strings.len() as u32;
        writer.ints[..4].copy_from_slice(&count.to_le_bytes());

        let mut buffer = writer.bytes.clone();
        buffer.resize(align(buffer.len(), 4), 0);
        buffer.extend_from_slice(&writer.ints);
        buffer.resize(align(buffer.len(), 8), 0);
        buffer.extend_from_slice(&writer.eights);
        let strings_at = buffer.len();
        for text in &writer.strings {
            buffer.extend_from_slice(text.as_bytes());
            buffer.push(0);
        }
        buffer.extend_from_slice(&writer.kinds);
        let strings_and_kinds = buffer.len() - strings_at;
        buffer.extend_from_slice(&TRAILER.to_le_bytes());

        let packed = match u32_at(&self.header, at::COMPRESSION)? {
            UNPACKED => buffer.clone(),
            _ => lz4::pack(&buffer),
        };
        let too_big = |_| Error::format(WHAT, "it would be too big");
        let mut out = self.header.clone();
        let numbers = [
            (at::BYTES, writer.bytes.len()),
            (at::INTS, writer.ints.len() / 4),
            (at::EIGHTS, writer.eights.len() / 8),
            (at::STRINGS_AND_KINDS, strings_and_kinds),
            (at::SIZE, buffer.len()),
            (at::PACKED, packed.len()),
        ];
        for (at, number) in numbers {
            out[at..at + 4].copy_from_slice(&u32::try_from(number).map_err(too_big)?.to_le_bytes());
        }
        for (at, number) in [(at::OBJECTS, writer.objects), (at::ARRAYS, writer.arrays)] {
            out[at..at + 2].copy_from_slice(&(number as u16).to_le_bytes());
        }
        out.extend_from_slice(&packed);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{object, text};
    use super::*;

    fn system() -> Value {
        object(&[
            ("_class", text("CParticleSystemDefinition")),
            ("m_nMaxParticles", Value::Plain(kind::INT_ZERO)),
            ("m_nBehaviorVersion", Value::Int(kind::INT32, 12)),
            ("m_flCullRadius", Value::Double(-1.0)),
            ("m_bShouldSort", Value::Plain(kind::FALSE)),
            ("m_bOn", Value::Bool(true)),
            ("m_hSnapshot", Value::Plain(kind::NULL)),
            ("m_nBig", Value::Int(kind::INT64, 1 << 40)),
            ("m_nMask", Value::Int(kind::UINT32, 0xFFFF_FFFF)),
            ("m_none", Value::String(None)),
            (
                "m_Children",
                Value::Typed(LONG_ARRAY, kind::OBJECT, vec![
                    object(&[("m_ChildRef", Value::Marked(1, Box::new(text("particles/a/ring.vpcf"))))]),
                    object(&[("m_ChildRef", Value::Marked(1, Box::new(text("particles/a/glow.vpcf")))), ("m_flDelay", Value::Plain(kind::DOUBLE_ONE))]),
                ]),
            ),
            ("m_vecOffset", Value::Typed(kind::SHORT_ARRAY, kind::DOUBLE, vec![Value::Double(1.0), Value::Double(0.0), Value::Double(0.5)])),
            ("m_Operators", Value::Array(vec![object(&[("_class", text("C_OP_Decay"))]), Value::Plain(kind::INT_ONE), Value::Typed(LONG_ARRAY, kind::INT32, vec![])])),
        ])
    }

    #[test]
    fn values_written_and_read_again_are_the_same_in_each_version() {
        for (version, compression) in [(2, LZ4), (3, UNPACKED), (4, LZ4), (4, UNPACKED)] {
            let block = Document::made(version, compression, system()).write().unwrap();
            assert!(is_earlier(&block));
            let mut read_back = read(&block).unwrap();
            assert_eq!(read_back.root, system(), "version {version}");
            let mut plain = read_back.clone();
            plain.not_compressed();
            let plain = plain.write().unwrap();
            assert_eq!(u32_at(&plain, at::COMPRESSION).unwrap(), UNPACKED);
            assert_eq!(u32_at(&plain, at::SIZE).unwrap(), u32_at(&plain, at::PACKED).unwrap());
            assert_eq!(read(&plain).unwrap().root, system(), "version {version}");
            if compression == UNPACKED {
                assert_eq!(plain, block);
            }
            read_back = read(&block).unwrap();
            assert_eq!(read_back.write().unwrap(), block, "version {version}");
        }
    }

    #[test]
    fn the_header_says_how_much_of_each_there_is() {
        let block = Document::made(2, UNPACKED, system()).write().unwrap();
        let number = |at: usize| u32_at(&block, at).unwrap() as usize;
        assert_eq!(number(at::BYTES), 1 + 1, "a yes, and the length of the short array");
        assert_eq!(number(at::EIGHTS), 1 + 1 + 3, "two numbers and the three of the offset");
        assert_eq!(u16::from_le_bytes([block[at::OBJECTS], block[at::OBJECTS + 1]]), 4, "objects in typed arrays count");
        assert_eq!(u16::from_le_bytes([block[at::ARRAYS], block[at::ARRAYS + 1]]), 4, "arrays of every kind");
        assert_eq!(number(at::SIZE), number(at::PACKED));
        assert_eq!(block.len(), 64 + number(at::SIZE));
        assert_eq!(&block[block.len() - 4..], &TRAILER.to_le_bytes());
        assert!(block.windows(2).any(|pair| pair == [kind::STRING | MARK, 1]));
    }

    #[test]
    fn a_new_string_goes_to_the_end_of_the_list() {
        let block = Document::made(2, LZ4, system()).write().unwrap();
        let mut document = read(&block).unwrap();
        let before = document.strings.clone();
        let Value::Object(members) = &mut document.root else { panic!() };
        members[0].1 = text("something new");
        let again = read(&document.write().unwrap()).unwrap();
        assert_eq!(again.strings[..before.len()], before[..]);
        assert_eq!(again.strings.last().map(String::as_str), Some("something new"));
        assert_eq!(again.root.get("_class"), Some(&text("something new")));
    }

    #[test]
    fn values_of_another_version_or_cut_short_are_refused() {
        let block = Document::made(2, LZ4, system()).write().unwrap();
        assert!(read(&block[..block.len() - 1]).is_err());
        let mut fifth = block.clone();
        fifth[0] = 5;
        assert!(!is_earlier(&fifth) && read(&fifth).is_err());
        let mut with_blocks = block.clone();
        with_blocks[at::BLOCKS] = 1;
        assert!(read(&with_blocks).is_err(), "blocks of bytes are not read");
        let mut packed_otherwise = block;
        packed_otherwise[at::COMPRESSION] = 2;
        assert!(read(&packed_otherwise).is_err());
        let mut short = vec![2, b'3', b'V', b'K'];
        short.resize(60, 0);
        assert!(unpacked(&short).is_err() && read(&short).is_err());
    }
}
