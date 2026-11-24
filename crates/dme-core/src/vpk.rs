use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::{fsutil, hash};

const MAGIC: u32 = 0x55AA_1234;
const EMBEDDED: u16 = 0x7FFF;
const ENTRY_LEN: usize = 18;
const TERMINATOR: u16 = 0xFFFF;
const OTHER_MD5_LEN: usize = 48;
const DATA_ALIGN: usize = 16;
const MAX_CHUNK: usize = 100 * 1024 * 1024;
const MD5_CHUNK: usize = 1024 * 1024;

pub struct Archive {
    dir_path: PathBuf,
    data: Vec<u8>,
    version: u32,
    header_len: usize,
    tree_len: usize,
    embedded_len: usize,
    archive_md5_len: usize,
    other_md5_len: usize,
    entries: Vec<Entry>,
    index: Vec<(u64, u32)>,
}

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    ext: Text,
    dir: Text,
    name: Text,
    preload_at: u32,
    preload_len: u16,
    archive: u16,
    offset: u32,
    length: u32,
    crc: u32,
}

#[derive(Clone, Copy, Debug)]
struct Text {
    at: u32,
    len: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Checksums {
    pub tree: bool,
    pub archive_section: bool,
    pub whole_file: bool,
}

impl Checksums {
    pub fn all_ok(&self) -> bool {
        self.tree && self.archive_section && self.whole_file
    }
}

impl Entry {
    pub fn size(&self) -> usize {
        self.preload_len as usize + self.length as usize
    }

    pub fn crc(&self) -> u32 {
        self.crc
    }

    pub fn is_embedded(&self) -> bool {
        self.archive == EMBEDDED
    }

    pub fn archive_index(&self) -> u16 {
        self.archive
    }

    pub fn offset(&self) -> u32 {
        self.offset
    }
}

impl Archive {
    pub fn dir_path(&self) -> &Path {
        &self.dir_path
    }

    pub fn open(dir_path: &Path) -> Result<Archive> {
        Archive::from_bytes(dir_path.to_path_buf(), fsutil::read(dir_path)?)
    }

    pub fn from_bytes(dir_path: PathBuf, data: Vec<u8>) -> Result<Archive> {
        let what = dir_path.display().to_string();
        let bad = |detail: &str| Error::format(what.clone(), detail);

        if data.len() < 12 {
            return Err(bad("the file is too short to be a VPK"));
        }
        if u32_at(&data, 0) != MAGIC {
            return Err(bad("the file does not start with the VPK magic number"));
        }
        let version = u32_at(&data, 4);
        let tree_len = u32_at(&data, 8) as usize;
        let (header_len, sections) = match version {
            1 => (12, None),
            2 => {
                if data.len() < 28 {
                    return Err(bad("the version 2 header is cut short"));
                }
                let sizes = [
                    u32_at(&data, 12) as usize,
                    u32_at(&data, 16) as usize,
                    u32_at(&data, 20) as usize,
                ];
                (28, Some(sizes))
            }
            _ => return Err(bad(&format!("VPK version {version} is not supported"))),
        };
        let tree_end = header_len + tree_len;
        if tree_end > data.len() {
            return Err(bad("the file list runs past the end of the file"));
        }
        let [embedded_len, archive_md5_len, other_md5_len] =
            sections.unwrap_or([data.len() - tree_end, 0, 0]);

        let entries = parse_tree(&data, header_len, tree_end).ok_or_else(|| bad("the file list is damaged"))?;

        let mut index: Vec<(u64, u32)> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let hash = hash_parts(text(&data, e.dir), text(&data, e.name), text(&data, e.ext));
                (hash, i as u32)
            })
            .collect();
        index.sort_unstable();

        Ok(Archive {
            dir_path,
            data,
            version,
            header_len,
            tree_len,
            embedded_len,
            archive_md5_len,
            other_md5_len,
            entries,
            index,
        })
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn path_of(&self, entry: &Entry) -> String {
        let (dir, name, ext) = (
            text(&self.data, entry.dir),
            text(&self.data, entry.name),
            text(&self.data, entry.ext),
        );
        let mut out = String::with_capacity(dir.len() + name.len() + ext.len() + 2);
        if !dir.is_empty() {
            out.push_str(&String::from_utf8_lossy(dir));
            out.push('/');
        }
        out.push_str(&String::from_utf8_lossy(name));
        if !ext.is_empty() {
            out.push('.');
            out.push_str(&String::from_utf8_lossy(ext));
        }
        out
    }

    pub fn named(&self, name: &str, ext: &str) -> Vec<String> {
        let (name, ext) = (name.to_ascii_lowercase(), ext.to_ascii_lowercase());
        let fits = |entry: &&Entry| same_part(text(&self.data, entry.name), name.as_bytes()) && same_part(text(&self.data, entry.ext), ext.as_bytes());
        self.entries.iter().filter(fits).map(|entry| self.path_of(entry)).collect()
    }

    pub fn find(&self, path: &str) -> Option<&Entry> {
        let (dir, name, ext) = split_path(path);
        let (dir, name, ext) = (dir.as_bytes(), name.as_bytes(), ext.as_bytes());
        let wanted = hash_parts(dir, name, ext);
        let first = self.index.partition_point(|&(h, _)| h < wanted);
        self.index[first..]
            .iter()
            .take_while(|&&(h, _)| h == wanted)
            .map(|&(_, i)| &self.entries[i as usize])
            .find(|e| {
                same_part(text(&self.data, e.dir), dir)
                    && same_part(text(&self.data, e.name), name)
                    && same_part(text(&self.data, e.ext), ext)
            })
    }

    pub fn contains(&self, path: &str) -> bool {
        self.find(path).is_some()
    }

    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        self.find(path).map(|e| self.read_entry(e)).transpose()
    }

    pub fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>> {
        self.read_entry_with(entry, &mut |chunk: &Path| File::open(chunk))
    }

    pub fn read_keeping_open(&self, path: &str, kept: &OpenFiles) -> Result<Option<Vec<u8>>> {
        let Some(entry) = self.find(path) else { return Ok(None) };
        let mut open = kept.0.borrow_mut();
        let mut reuse = |chunk: &Path| match open.get(chunk) {
            Some(file) => file.try_clone(),
            None => {
                let file = File::open(chunk)?;
                open.insert(chunk.to_path_buf(), file.try_clone()?);
                Ok(file)
            }
        };
        self.read_entry_with(entry, &mut reuse).map(Some)
    }

    fn read_entry_with(&self, entry: &Entry, open: &mut dyn FnMut(&Path) -> std::io::Result<File>) -> Result<Vec<u8>> {
        let what = || self.dir_path.display().to_string();
        let preload_at = entry.preload_at as usize;
        let preload_len = entry.preload_len as usize;
        let length = entry.length as usize;
        let preload = preload_at
            .checked_add(preload_len)
            .and_then(|end| self.data.get(preload_at..end))
            .ok_or_else(|| Error::format(what(), "a file's preload data runs past the end of the file"))?;

        if length == 0 {
            return Ok(preload.to_vec());
        }
        if entry.archive == EMBEDDED {
            let start = self.header_len + self.tree_len + entry.offset as usize;
            let bytes = start
                .checked_add(length)
                .and_then(|end| self.data.get(start..end))
                .ok_or_else(|| Error::format(what(), "a file's data runs past the end of the file"))?;
            let mut out = Vec::with_capacity(preload_len + length);
            out.extend_from_slice(preload);
            out.extend_from_slice(bytes);
            Ok(out)
        } else {
            let chunk = self.chunk_path(entry.archive)?;
            let mut file = open(&chunk).map_err(|e| Error::io("open", &chunk, e))?;
            let file_len = file.metadata().map_err(|e| Error::io("read", &chunk, e))?.len();
            let end = u64::from(entry.offset).checked_add(u64::from(entry.length));
            if end.is_none_or(|end| end > file_len) {
                return Err(Error::format(what(), "a file's data runs past the end of its numbered data file"));
            }
            let total = preload_len.checked_add(length).ok_or_else(|| Error::format(what(), "a file is too large"))?;
            let mut out = Vec::with_capacity(total);
            out.extend_from_slice(preload);
            out.resize(total, 0);
            file.seek(SeekFrom::Start(entry.offset as u64))
                .and_then(|_| file.read_exact(&mut out[preload_len..]))
                .map_err(|e| Error::io("read", &chunk, e))?;
            Ok(out)
        }
    }

    fn chunk_path(&self, index: u16) -> Result<PathBuf> {
        chunk_path_for(&self.dir_path, index)
    }

    pub fn verify_checksums(&self) -> Option<Checksums> {
        if self.version != 2 || self.other_md5_len != OTHER_MD5_LEN {
            return None;
        }
        let tree_start = self.header_len;
        let tree_end = tree_start + self.tree_len;
        let archive_start = tree_end.checked_add(self.embedded_len)?;
        let other = archive_start.checked_add(self.archive_md5_len)?;
        let stored = self.data.get(other..other.checked_add(OTHER_MD5_LEN)?)?;
        Some(Checksums {
            tree: hash::md5(&self.data[tree_start..tree_end]) == stored[..16],
            archive_section: hash::md5(&self.data[archive_start..other]) == stored[16..32],
            whole_file: hash::md5(&self.data[..other + 32]) == stored[32..],
        })
    }
}

pub struct PackFile<'a> {
    pub path: &'a str,
    pub data: &'a [u8],
}

pub type NewFile = (String, Vec<u8>);

fn chunk_path_for(dir_path: &Path, index: u16) -> Result<PathBuf> {
    let stem = dir_stem(dir_path).ok_or_else(|| {
        Error::format(
            dir_path.display().to_string(),
            "the file name does not end in _dir.vpk, so its numbered data files cannot be found",
        )
    })?;
    Ok(dir_path.with_file_name(format!("{stem}_{index:03}.vpk")))
}

fn dir_stem(dir_path: &Path) -> Option<&str> {
    let name = dir_path.file_name()?.to_str()?;
    let at = name.len().checked_sub("_dir.vpk".len())?;
    (name.is_char_boundary(at) && name[at..].eq_ignore_ascii_case("_dir.vpk")).then(|| &name[..at])
}

pub fn package_is_complete(dir_path: &Path) -> bool {
    let Ok(mut file) = File::open(dir_path) else { return false };
    let mut head = [0u8; 28];
    let Ok(read) = file.read(&mut head) else { return false };
    if read < 12 || u32_at(&head, 0) != MAGIC {
        return false;
    }
    if u32_at(&head, 4) == 2 && read == 28 && u32_at(&head, 12) == 0 {
        return chunk_path_for(dir_path, 0).is_ok_and(|chunk| chunk.is_file());
    }
    true
}

type Groups<'a> = BTreeMap<String, BTreeMap<String, BTreeMap<String, &'a [u8]>>>;

fn group_files<'a>(files: &[PackFile<'a>]) -> Result<Groups<'a>> {
    let bad = |detail: String| Error::format("the mod package", detail);
    let mut groups: Groups<'a> = BTreeMap::new();
    for file in files {
        let (dir, name, ext) = split_path(file.path);
        if name.is_empty() || file.path.contains('\0') {
            return Err(bad(format!("{:?} is not a usable file path", file.path)));
        }
        if u32::try_from(file.data.len()).is_err() {
            return Err(bad(format!("{:?} is larger than 4 GB", file.path)));
        }
        let names = groups.entry(ext).or_default().entry(dir).or_default();
        if names.insert(name, file.data).is_some() {
            return Err(bad(format!("{:?} is listed twice", file.path)));
        }
    }
    Ok(groups)
}

pub fn validate(files: &[PackFile<'_>]) -> Result<()> {
    group_files(files)?;
    Ok(())
}

pub fn build(files: &[PackFile<'_>]) -> Result<Vec<u8>> {
    let bad = |detail: String| Error::format("the mod package", detail);
    let groups = group_files(files)?;

    let total: usize = files.iter().map(|f| f.data.len()).sum();
    let total_u32 = u32::try_from(total).map_err(|_| bad("the files are larger than 4 GB in total".into()))?;

    let mut tree = Vec::new();
    let mut offset = 0u32;
    for (ext, dirs) in &groups {
        push_cstr(&mut tree, ext);
        for (dir, names) in dirs {
            push_cstr(&mut tree, dir);
            for (name, data) in names {
                push_cstr(&mut tree, name);
                tree.extend_from_slice(&hash::crc32(data).to_le_bytes());
                tree.extend_from_slice(&0u16.to_le_bytes());
                tree.extend_from_slice(&EMBEDDED.to_le_bytes());
                tree.extend_from_slice(&offset.to_le_bytes());
                tree.extend_from_slice(&(data.len() as u32).to_le_bytes());
                tree.extend_from_slice(&TERMINATOR.to_le_bytes());
                offset += data.len() as u32;
            }
            tree.push(0);
        }
        tree.push(0);
    }
    tree.push(0);
    let tree_len = u32::try_from(tree.len()).map_err(|_| bad("the file list is too large".into()))?;

    let mut out = Vec::with_capacity(28 + tree.len() + total + OTHER_MD5_LEN);
    for field in [MAGIC, 2, tree_len, total_u32, 0, OTHER_MD5_LEN as u32, 0] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&tree);
    for names in groups.values().flat_map(|dirs| dirs.values()) {
        for data in names.values() {
            out.extend_from_slice(data);
        }
    }
    out.extend_from_slice(&hash::md5(&tree));
    out.extend_from_slice(&hash::md5(&[]));
    let whole = hash::md5(&out);
    out.extend_from_slice(&whole);
    Ok(out)
}

pub fn write_package(dir_path: &Path, files: &[PackFile<'_>]) -> Result<usize> {
    let bad = |detail: String| Error::format("the mod package", detail);
    let groups = group_files(files)?;
    chunk_path_for(dir_path, 0)?;

    let mut tree = Vec::new();
    let mut chunks: Vec<Vec<(u32, &[u8])>> = vec![Vec::new()];
    let mut archive_idx: u16 = 0;
    let mut offset: usize = 0;

    for (ext, dirs) in &groups {
        push_cstr(&mut tree, ext);
        for (dir, names) in dirs {
            push_cstr(&mut tree, dir);
            for (name, data) in names {
                let mut aligned = offset.div_ceil(DATA_ALIGN) * DATA_ALIGN;
                let needs_new_chunk = !chunks[archive_idx as usize].is_empty()
                    && (aligned.saturating_add(data.len()) > MAX_CHUNK || u32::try_from(aligned.saturating_add(data.len())).is_err());
                if needs_new_chunk {
                    archive_idx = archive_idx
                        .checked_add(1)
                        .filter(|&next| next < EMBEDDED)
                        .ok_or_else(|| bad("the package would have too many numbered data files".into()))?;
                    chunks.push(Vec::new());
                    aligned = 0;
                }
                let end = aligned
                    .checked_add(data.len())
                    .filter(|&end| u32::try_from(end).is_ok())
                    .ok_or_else(|| bad("a file is too large for a VPK chunk".into()))?;

                push_cstr(&mut tree, name);
                tree.extend_from_slice(&hash::crc32(data).to_le_bytes());
                tree.extend_from_slice(&0u16.to_le_bytes());
                tree.extend_from_slice(&archive_idx.to_le_bytes());
                tree.extend_from_slice(&(aligned as u32).to_le_bytes());
                tree.extend_from_slice(&(data.len() as u32).to_le_bytes());
                tree.extend_from_slice(&TERMINATOR.to_le_bytes());

                chunks[archive_idx as usize].push((aligned as u32, data));
                offset = end;
            }
            tree.push(0);
        }
        tree.push(0);
    }
    tree.push(0);
    let tree_len = u32::try_from(tree.len()).map_err(|_| bad("the file list is too large".into()))?;

    let mut archive_md5 = Vec::new();
    let mut total_data_bytes = 0usize;
    let zeros = [0u8; DATA_ALIGN];
    for (index, entries) in chunks.iter().enumerate() {
        let chunk_path = chunk_path_for(dir_path, index as u16)?;
        let (written, records) = fsutil::write_atomic_with(&chunk_path, |out| {
            let mut records = Vec::new();
            let mut hash_buf = Vec::with_capacity(MD5_CHUNK.min(entries.iter().map(|(_, d)| d.len()).sum::<usize>().saturating_add(entries.len() * DATA_ALIGN)));
            let mut chunk_start = 0u32;
            let mut written = 0usize;
            let mut push = |bytes: &[u8], out: &mut std::io::BufWriter<File>| -> std::io::Result<()> {
                let mut rest = bytes;
                while !rest.is_empty() {
                    let take = (MD5_CHUNK - hash_buf.len()).min(rest.len());
                    hash_buf.extend_from_slice(&rest[..take]);
                    out.write_all(&rest[..take])?;
                    rest = &rest[take..];
                    if hash_buf.len() == MD5_CHUNK {
                        push_archive_md5(&mut records, index as u32, chunk_start, &hash_buf);
                        chunk_start += hash_buf.len() as u32;
                        hash_buf.clear();
                    }
                }
                Ok(())
            };
            for &(aligned, data) in entries {
                let pad = aligned as usize - written;
                if pad > 0 {
                    push(&zeros[..pad], out)?;
                    written += pad;
                }
                push(data, out)?;
                written += data.len();
            }
            if !hash_buf.is_empty() {
                push_archive_md5(&mut records, index as u32, chunk_start, &hash_buf);
            }
            Ok((written, records))
        })?;
        total_data_bytes += written;
        archive_md5.extend_from_slice(&records);
    }

    let archive_md5_len = u32::try_from(archive_md5.len()).map_err(|_| bad("the archive checksum section is too large".into()))?;
    let mut out = Vec::with_capacity(28 + tree.len() + archive_md5.len() + OTHER_MD5_LEN);
    for field in [MAGIC, 2, tree_len, 0, archive_md5_len, OTHER_MD5_LEN as u32, 0] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&tree);
    out.extend_from_slice(&archive_md5);
    out.extend_from_slice(&hash::md5(&tree));
    out.extend_from_slice(&hash::md5(&archive_md5));
    let whole = hash::md5(&out);
    out.extend_from_slice(&whole);
    fsutil::write_atomic(dir_path, &out)?;

    remove_stale_chunks(dir_path, chunks.len());
    Ok(out.len() + total_data_bytes)
}

fn push_archive_md5(out: &mut Vec<u8>, archive: u32, start: u32, bytes: &[u8]) {
    out.extend_from_slice(&archive.to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&hash::md5(bytes));
}

fn remove_stale_chunks(dir_path: &Path, keep: usize) {
    let (Some(parent), Some(stem)) = (dir_path.parent(), dir_stem(dir_path)) else { return };
    let prefix = format!("{}_", stem.to_ascii_lowercase());
    let Ok(entries) = std::fs::read_dir(parent) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let lower = name.to_ascii_lowercase();
        let Some(rest) = lower.strip_prefix(&prefix).and_then(|rest| rest.strip_suffix(".vpk")) else { continue };
        if rest.len() == 3 && rest.bytes().all(|b| b.is_ascii_digit()) && rest.parse::<usize>().is_ok_and(|n| n >= keep) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn push_cstr(tree: &mut Vec<u8>, part: &str) {
    tree.extend_from_slice(if part.is_empty() { b" " } else { part.as_bytes() });
    tree.push(0);
}

fn parse_tree(data: &[u8], tree_start: usize, tree_end: usize) -> Option<Vec<Entry>> {
    let mut pos = tree_start;
    let cstr = |pos: &mut usize| -> Option<Text> {
        let len = memchr::memchr(0, data.get(*pos..tree_end)?)?;
        let at = *pos;
        *pos += len + 1;
        Some(Text {
            at: u32::try_from(at).ok()?,
            len: u16::try_from(len).ok()?,
        })
    };

    let mut entries = Vec::new();
    loop {
        let ext = cstr(&mut pos)?;
        if ext.len == 0 {
            return Some(entries);
        }
        loop {
            let dir = cstr(&mut pos)?;
            if dir.len == 0 {
                break;
            }
            loop {
                let name = cstr(&mut pos)?;
                if name.len == 0 {
                    break;
                }
                let raw = data.get(pos..pos.checked_add(ENTRY_LEN)?)?;
                if pos + ENTRY_LEN > tree_end || u16_at(raw, 16) != TERMINATOR {
                    return None;
                }
                let preload_len = u16_at(raw, 4);
                let preload_at = pos + ENTRY_LEN;
                pos = preload_at + preload_len as usize;
                if pos > tree_end {
                    return None;
                }
                entries.push(Entry {
                    ext: blank_if_space(data, ext),
                    dir: blank_if_space(data, dir),
                    name,
                    preload_at: u32::try_from(preload_at).ok()?,
                    preload_len,
                    archive: u16_at(raw, 6),
                    offset: u32_at(raw, 8),
                    length: u32_at(raw, 12),
                    crc: u32_at(raw, 0),
                });
            }
        }
    }
}

fn blank_if_space(data: &[u8], part: Text) -> Text {
    if text(data, part) == b" " {
        Text { at: part.at, len: 0 }
    } else {
        part
    }
}

fn text(data: &[u8], part: Text) -> &[u8] {
    &data[part.at as usize..part.at as usize + part.len as usize]
}

fn split_path(path: &str) -> (String, String, String) {
    let normal = path.replace('\\', "/").to_ascii_lowercase();
    let normal = normal.trim_start_matches('/');
    let (dir, file) = normal.rsplit_once('/').unwrap_or(("", normal));
    let (name, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    (dir.to_string(), name.to_string(), ext.to_string())
}

fn same_part(stored: &[u8], wanted: &[u8]) -> bool {
    stored.len() == wanted.len() && stored.iter().zip(wanted).all(|(&a, &b)| fold(a) == b)
}

fn fold(byte: u8) -> u8 {
    if byte == b'\\' { b'/' } else { byte.to_ascii_lowercase() }
}

fn hash_parts(dir: &[u8], name: &[u8], ext: &[u8]) -> u64 {
    fn feed(mut hash: u64, bytes: &[u8]) -> u64 {
        for &b in bytes {
            hash ^= fold(b) as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }
    let mut hash = 0xCBF2_9CE4_8422_2325;
    if !dir.is_empty() {
        hash = feed(hash, dir);
        hash = feed(hash, b"/");
    }
    hash = feed(hash, name);
    if !ext.is_empty() {
        hash = feed(hash, b".");
        hash = feed(hash, ext);
    }
    hash
}

fn u16_at(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

#[derive(Default)]
pub struct OpenFiles(RefCell<HashMap<PathBuf, File>>);

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        build(&[
            PackFile { path: "scripts/items/items_game.txt", data: b"hello items" },
            PackFile { path: "scripts/items/other.txt", data: b"" },
            PackFile { path: "models/a/b.vmdl_c", data: &[1, 2, 3, 4, 5] },
            PackFile { path: "rootfile.txt", data: b"at the root" },
            PackFile { path: "folder/noextension", data: b"no ext" },
        ])
        .unwrap()
    }

    fn open(bytes: Vec<u8>) -> Archive {
        Archive::from_bytes(PathBuf::from("test_dir.vpk"), bytes).unwrap()
    }

    #[test]
    fn what_is_written_reads_back() {
        let archive = open(sample());
        assert_eq!(archive.version(), 2);
        assert_eq!(archive.len(), 5);
        assert_eq!(archive.read("scripts/items/items_game.txt").unwrap().unwrap(), b"hello items");
        assert_eq!(archive.read("scripts/items/other.txt").unwrap().unwrap(), b"");
        assert_eq!(archive.read("models/a/b.vmdl_c").unwrap().unwrap(), [1, 2, 3, 4, 5]);
        assert_eq!(archive.read("rootfile.txt").unwrap().unwrap(), b"at the root");
        assert_eq!(archive.read("folder/noextension").unwrap().unwrap(), b"no ext");
        assert!(archive.read("scripts/items/missing.txt").unwrap().is_none());
        assert!(archive.read("items_game.txt").unwrap().is_none());
    }

    #[test]
    fn lookup_ignores_case_and_slash_direction() {
        let archive = open(sample());
        assert!(archive.contains("Scripts\\Items\\ITEMS_GAME.TXT"));
        assert!(archive.contains("/models/a/b.vmdl_c"));
        assert!(!archive.contains("models/a/b.vmdl"));
    }

    #[test]
    fn entries_report_path_size_and_crc() {
        let archive = open(sample());
        let entry = archive.find("models/a/b.vmdl_c").unwrap();
        assert_eq!(archive.path_of(entry), "models/a/b.vmdl_c");
        assert_eq!(entry.size(), 5);
        assert_eq!(entry.crc(), hash::crc32(&[1, 2, 3, 4, 5]));
        assert!(entry.is_embedded());

        let mut paths: Vec<String> = archive.entries().iter().map(|e| archive.path_of(e)).collect();
        paths.sort();
        assert_eq!(
            paths,
            ["folder/noextension", "models/a/b.vmdl_c", "rootfile.txt", "scripts/items/items_game.txt", "scripts/items/other.txt"]
        );
    }

    #[test]
    fn written_checksums_verify_and_notice_damage() {
        let bytes = sample();
        assert_eq!(open(bytes.clone()).verify_checksums().unwrap(), Checksums { tree: true, archive_section: true, whole_file: true });

        let mut damaged = bytes.clone();
        let at = damaged.len() - OTHER_MD5_LEN - 1;
        damaged[at] ^= 0xFF;
        assert_eq!(open(damaged).verify_checksums().unwrap(), Checksums { tree: true, archive_section: true, whole_file: false });
    }

    #[test]
    fn output_is_the_same_whatever_the_input_order() {
        let a = build(&[PackFile { path: "a/x.txt", data: b"1" }, PackFile { path: "b/y.txt", data: b"2" }]).unwrap();
        let b = build(&[PackFile { path: "b/y.txt", data: b"2" }, PackFile { path: "a/x.txt", data: b"1" }]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn build_rejects_duplicates_and_empty_names() {
        assert!(build(&[PackFile { path: "a.txt", data: b"" }, PackFile { path: "A.TXT", data: b"" }]).is_err());
        assert!(build(&[PackFile { path: "folder/", data: b"" }]).is_err());
    }

    #[test]
    fn damaged_files_are_errors_not_panics() {
        let good = sample();
        for cut in [0, 4, 11, 12, 20, 27, 28, 40, good.len() / 2] {
            let _ = Archive::from_bytes(PathBuf::from("cut_dir.vpk"), good[..cut].to_vec());
        }
        let mut wrong_magic = good.clone();
        wrong_magic[0] = 0;
        assert!(Archive::from_bytes(PathBuf::from("x_dir.vpk"), wrong_magic).is_err());
        let mut huge_tree = good.clone();
        huge_tree[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Archive::from_bytes(PathBuf::from("x_dir.vpk"), huge_tree).is_err());
        let mut no_terminator = good.clone();
        let tree_end = 28 + u32_at(&good, 8) as usize;
        no_terminator[tree_end - 1] = b'x';
        assert!(Archive::from_bytes(PathBuf::from("x_dir.vpk"), no_terminator).is_err());
    }

    #[test]
    fn data_in_numbered_files_is_read_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut tree = Vec::new();
        for part in ["txt", "docs", "readme"] {
            tree.extend_from_slice(part.as_bytes());
            tree.push(0);
        }
        tree.extend_from_slice(&0u32.to_le_bytes());
        tree.extend_from_slice(&2u16.to_le_bytes());
        tree.extend_from_slice(&3u16.to_le_bytes());
        tree.extend_from_slice(&4u32.to_le_bytes());
        tree.extend_from_slice(&5u32.to_le_bytes());
        tree.extend_from_slice(&TERMINATOR.to_le_bytes());
        tree.extend_from_slice(b"PR");
        tree.extend_from_slice(&[0, 0, 0]);
        let mut bytes = Vec::new();
        for field in [MAGIC, 1, tree.len() as u32] {
            bytes.extend_from_slice(&field.to_le_bytes());
        }
        bytes.extend_from_slice(&tree);

        let dir_path = dir.path().join("pak01_dir.vpk");
        std::fs::write(&dir_path, &bytes).unwrap();
        std::fs::write(dir.path().join("pak01_003.vpk"), b"....HELLO....").unwrap();

        let archive = Archive::open(&dir_path).unwrap();
        assert_eq!(archive.version(), 1);
        let entry = archive.find("docs/readme.txt").unwrap();
        assert!(!entry.is_embedded());
        assert_eq!(entry.archive_index(), 3);
        assert_eq!(archive.read_entry(entry).unwrap(), b"PRHELLO");
        assert!(archive.verify_checksums().is_none());
    }

    #[test]
    fn write_package_streams_aligned_numbered_data_files_with_archive_md5() {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().join("pak01_dir.vpk");
        let stale = dir.path().join("pak01_001.vpk");
        std::fs::write(&stale, b"stale chunk").unwrap();
        let big = vec![0xABu8; MD5_CHUNK + 37];
        let written = write_package(
            &dir_path,
            &[
                PackFile { path: "scripts/items/items_game.txt", data: b"hello items" },
                PackFile { path: "scripts/items/empty.txt", data: b"" },
                PackFile { path: "models/a/big.vmdl_c", data: &big },
                PackFile { path: "models/a/small.vmdl_c", data: &[1, 2, 3, 4, 5] },
            ],
        )
        .unwrap();

        let chunk_path = dir.path().join("pak01_000.vpk");
        assert!(package_is_complete(&dir_path));
        assert!(!stale.exists(), "higher-numbered chunks from an earlier package are cleaned up");
        assert_eq!(written as u64, std::fs::metadata(&dir_path).unwrap().len() + std::fs::metadata(&chunk_path).unwrap().len());

        let archive = Archive::open(&dir_path).unwrap();
        assert_eq!(archive.version(), 2);
        assert_eq!(archive.embedded_len, 0);
        assert_eq!(archive.archive_md5_len, 2 * 28, "1 MB record plus the remainder");
        assert!(archive.verify_checksums().unwrap().all_ok());
        for entry in archive.entries() {
            assert!(!entry.is_embedded());
            assert_eq!(entry.archive_index(), 0);
            assert_eq!(entry.offset() as usize % DATA_ALIGN, 0, "every entry is 16-byte aligned");
        }
        assert_eq!(archive.read("scripts/items/items_game.txt").unwrap().unwrap(), b"hello items");
        assert_eq!(archive.read("scripts/items/empty.txt").unwrap().unwrap(), b"");
        assert_eq!(archive.read("models/a/big.vmdl_c").unwrap().unwrap(), big);
        assert_eq!(archive.read("models/a/small.vmdl_c").unwrap().unwrap(), [1, 2, 3, 4, 5]);

        let chunk_bytes = std::fs::read(&chunk_path).unwrap();
        let section_start = archive.header_len + archive.tree_len;
        let section = &archive.data[section_start..section_start + archive.archive_md5_len];
        for record in section.as_chunks::<28>().0 {
            let idx = u32_at(record, 0);
            let start = u32_at(record, 4) as usize;
            let count = u32_at(record, 8) as usize;
            assert_eq!(idx, 0);
            assert_eq!(hash::md5(&chunk_bytes[start..start + count]), record[12..28]);
        }

        std::fs::remove_file(&chunk_path).unwrap();
        assert!(!package_is_complete(&dir_path));
    }
}
