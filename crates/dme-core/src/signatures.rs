use crate::error::{Error, Result};
use crate::hash;

const WHAT: &str = "dota.signatures";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    pub path: &'a str,
    pub sha1: &'a str,
    pub crc: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct Signatures<'a> {
    pub valve: &'a str,
    pub extra: &'a str,
    pub newline: &'static str,
}

impl<'a> Signatures<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Signatures<'a>> {
        let text = std::str::from_utf8(bytes).map_err(|_| Error::format(WHAT, "the file is not plain text"))?;
        let digest_at = if text.starts_with("DIGEST:") {
            0
        } else {
            text.find("\nDIGEST:")
                .map(|i| i + 1)
                .ok_or_else(|| Error::format(WHAT, "there is no DIGEST line"))?
        };
        let digest_end = text[digest_at..]
            .find('\n')
            .map(|i| digest_at + i + 1)
            .ok_or_else(|| Error::format(WHAT, "the DIGEST line does not end with a line break"))?;
        let (valve, extra) = text.split_at(digest_end);
        Ok(Signatures {
            valve,
            extra,
            newline: if valve.contains("\r\n") { "\r\n" } else { "\n" },
        })
    }

    pub fn valve_lines(&self) -> impl Iterator<Item = Line<'a>> {
        self.valve.lines().filter_map(parse_line)
    }

    pub fn extra_lines(&self) -> impl Iterator<Item = Line<'a>> {
        self.extra.lines().filter_map(parse_line)
    }

    pub fn valve_line_for(&self, file_name: &str) -> Option<Line<'a>> {
        self.valve_lines().find(|line| names_file(line.path, file_name))
    }

    pub fn without_extra(&self) -> String {
        self.valve.to_string()
    }

    pub fn with_extra_line(&self, line: &str) -> String {
        format!("{}{}{}", self.valve, line, self.newline)
    }
}

pub fn make_line(path: &str, contents: &[u8]) -> String {
    format!("{path}~SHA1:{};CRC:{}", hash::sha1_hex(contents), hash::signature_crc_hex(contents))
}

pub fn matches(line: &Line<'_>, contents: &[u8]) -> bool {
    line.sha1.eq_ignore_ascii_case(&hash::sha1_hex(contents))
        && line.crc.eq_ignore_ascii_case(&hash::signature_crc_hex(contents))
}

fn parse_line(line: &str) -> Option<Line<'_>> {
    let (path, hashes) = line.trim_end().split_once("~SHA1:")?;
    let (sha1, crc) = hashes.split_once(";CRC:")?;
    let is_hex = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
    (is_hex(sha1, 40) && is_hex(crc, 8)).then_some(Line { path, sha1, crc })
}

fn names_file(path: &str, file_name: &str) -> bool {
    path.rsplit(['\\', '/'])
        .next()
        .is_some_and(|last| last.eq_ignore_ascii_case(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VANILLA: &str = "...\\..\\..\\dota\\bin\\win64\\client.dll~SHA1:F375EEF89A0AFAAE435D32DD59C3A3963A9E854C;CRC:8F238F8B\r\n...\\..\\..\\dota\\gameinfo.gi~SHA1:088FE9788E2342E7AF671A5AD136FB143A6B5A09;CRC:C18D898A\r\n...\\..\\..\\dota\\gameinfo_branchspecific.gi~SHA1:224B6F879D00D419B6A10FE07B4DDB3D827FD31F;CRC:7B89F784\r\n...\\worldrenderer.dll~SHA1:D50FB480EA3BFD31503B54FC8659C67C03921D26;CRC:0F2FDACE\r\nDIGEST:3229731524E321D4\r\n";

    #[test]
    fn untouched_file_has_no_extra_part() {
        let sig = Signatures::parse(VANILLA.as_bytes()).unwrap();
        assert_eq!(sig.valve, VANILLA);
        assert_eq!(sig.extra, "");
        assert_eq!(sig.newline, "\r\n");
        assert_eq!(sig.valve_lines().count(), 4);
        assert_eq!(sig.extra_lines().count(), 0);
    }

    #[test]
    fn finds_valves_declaration_by_file_name() {
        let sig = Signatures::parse(VANILLA.as_bytes()).unwrap();
        let line = sig.valve_line_for("gameinfo_branchspecific.gi").unwrap();
        assert_eq!(line.path, "...\\..\\..\\dota\\gameinfo_branchspecific.gi");
        assert_eq!(line.sha1, "224B6F879D00D419B6A10FE07B4DDB3D827FD31F");
        assert_eq!(line.crc, "7B89F784");
        assert_eq!(sig.valve_line_for("gameinfo.gi").unwrap().crc, "C18D898A");
        assert!(sig.valve_line_for("missing.gi").is_none());
    }

    #[test]
    fn adding_a_line_and_removing_it_gives_the_original_bytes() {
        let sig = Signatures::parse(VANILLA.as_bytes()).unwrap();
        let line = make_line("...\\..\\..\\dota\\gameinfo_branchspecific.gi", b"edited file");
        let edited = sig.with_extra_line(&line);
        assert!(edited.starts_with(VANILLA));
        assert!(edited.ends_with("\r\n"));

        let again = Signatures::parse(edited.as_bytes()).unwrap();
        assert_eq!(again.without_extra(), VANILLA);
        let ours: Vec<Line> = again.extra_lines().collect();
        assert_eq!(ours.len(), 1);
        assert!(matches(&ours[0], b"edited file"));
        assert!(!matches(&ours[0], b"another file"));

        let replaced = again.with_extra_line(&make_line(ours[0].path, b"second edit"));
        assert_eq!(Signatures::parse(replaced.as_bytes()).unwrap().extra_lines().count(), 1);
    }

    #[test]
    fn a_declaration_has_the_documented_shape() {
        let line = make_line("...\\x.gi", b"123456789");
        assert_eq!(line, "...\\x.gi~SHA1:F7C3BC1D808E04732ADF679965CCC34CA7AE3441;CRC:2639F4CB");
        assert_eq!(parse_line(&line).unwrap().path, "...\\x.gi");
    }

    #[test]
    fn files_without_a_proper_digest_line_are_refused() {
        assert!(Signatures::parse(b"a~SHA1:00;CRC:00\r\n").is_err());
        assert!(Signatures::parse(b"...\\a.dll~SHA1:F375EEF89A0AFAAE435D32DD59C3A3963A9E854C;CRC:8F238F8B\r\nDIGEST:AB").is_err());
        assert!(Signatures::parse(&[0xFF, 0xFE, 0x00]).is_err());
        assert!(Signatures::parse(b"DIGEST:AB\n").is_ok());
    }

    #[test]
    fn unix_line_breaks_are_kept() {
        let sig = Signatures::parse(b"...\\a.gi~SHA1:F375EEF89A0AFAAE435D32DD59C3A3963A9E854C;CRC:8F238F8B\nDIGEST:AB\n").unwrap();
        assert_eq!(sig.newline, "\n");
        assert!(sig.with_extra_line("x").ends_with("DIGEST:AB\nx\n"));
    }
}
