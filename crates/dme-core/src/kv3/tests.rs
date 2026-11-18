use super::*;

fn like() -> Vec<u8> {
    let mut block = MAGIC.to_vec();
    block.extend_from_slice(&[7; 16]);
    block.extend_from_slice(&LZ4.to_le_bytes());
    block.extend_from_slice(&0x4000_0000u32.to_le_bytes());
    block.resize(HEADER, 0);
    block
}

pub(crate) fn text(value: &str) -> Value {
    Value::String(Some(value.to_string()))
}

pub(crate) fn object(members: &[(&str, Value)]) -> Value {
    Value::Object(members.iter().map(|(name, value)| (name.to_string(), value.clone())).collect())
}

fn events() -> Value {
    object(&[
        (
            "Hero_Axe.Attack",
            object(&[
                ("type", text("dota_src1_3d")),
                ("volume", Value::Double(0.6)),
                ("pitch", Value::Plain(kind::DOUBLE_ONE)),
                ("delay", Value::Plain(kind::DOUBLE_ZERO)),
                ("soundevent_layer2", text("Hero_Axe.Attack.Impact")),
                ("vsnd_files", Value::Typed(kind::SHORT_ARRAY, kind::STRING, vec![text("sounds/a.vsnd"), text("sounds/b.vsnd")])),
                ("position", Value::Typed(kind::SIDE_ARRAY, kind::DOUBLE, vec![Value::Double(1.5), Value::Double(-2.0), Value::Double(3.0)])),
            ]),
        ),
        (
            "Hero_Axe.Attack.Impact",
            object(&[
                ("type", text("dota_update_default")),
                ("on", Value::Plain(kind::TRUE)),
                ("off", Value::Plain(kind::FALSE)),
                ("count", Value::Int(kind::INT32, -3)),
                ("big", Value::Int(kind::INT64, 1 << 40)),
                ("none", Value::String(None)),
                ("list", Value::Array(vec![Value::Plain(kind::INT_ONE), text("sounds/a.vsnd"), Value::Plain(kind::NULL)])),
                ("nested", Value::Typed(kind::SHORT_ARRAY, kind::SHORT_ARRAY, vec![Value::Typed(kind::SHORT_ARRAY, kind::DOUBLE, vec![Value::Double(0.25)])])),
            ]),
        ),
    ])
}

#[test]
fn values_written_and_read_again_are_the_same() {
    let block = write(&events(), &like()).unwrap();
    assert_eq!(read(&block).unwrap(), events());
}

#[test]
fn values_written_twice_are_the_same_bytes() {
    let block = write(&events(), &like()).unwrap();
    assert_eq!(write(&read(&block).unwrap(), &block).unwrap(), block);
}

#[test]
fn the_header_says_how_much_of_each_there_is() {
    let block = write(&events(), &like()).unwrap();
    let number = |field: usize| u32_at(&block, 20 + 4 * field).unwrap();
    assert_eq!(&block[..20], &like()[..20], "the magic and the format's id are kept");
    assert_eq!(number(Header::COMPRESSION), LZ4);
    assert_eq!(number(1), 0x4000_0000);
    assert_eq!(number(Header::FIRST_INTS), 1);
    assert_eq!(number(Header::FIRST_EIGHTS), 3, "the position's numbers");
    assert_eq!(number(Header::OBJECTS), 3);
    assert_eq!(number(Header::ROOM), 3 | 4 << 16, "three objects and four arrays with a byte for their length");
    assert_eq!(number(Header::BYTES), 4);
    assert_eq!(number(Header::SHORT_ARRAYS), 3);
    assert_eq!(number(Header::SHORT_ARRAY_ITEMS), 2 + 1 + 1);
    assert_eq!(number(Header::KINDS) - number(Header::VALUES), 4, "one kind for the items of each typed array");
    assert_eq!(number(Header::TOTAL), number(Header::FIRST_SIZE) + number(Header::SECOND_SIZE));
    assert_eq!(number(Header::TOTAL_PACKED), number(Header::FIRST_PACKED) + number(Header::SECOND_PACKED));
    assert_eq!(block.len(), HEADER + number(Header::TOTAL_PACKED) as usize);
}

#[test]
fn a_string_is_listed_once_where_it_is_first_used() {
    let block = write(&events(), &like()).unwrap();
    let first = lz4::unpack(&block[HEADER..HEADER + u32_at(&block, 20 + 4 * Header::FIRST_PACKED).unwrap() as usize], u32_at(&block, 20 + 4 * Header::FIRST_SIZE).unwrap() as usize).unwrap();
    let strings = &first[..u32_at(&block, 20 + 4 * Header::STRING_BYTES).unwrap() as usize];
    let listed: Vec<&[u8]> = strings.split(|&byte| byte == 0).filter(|text| !text.is_empty()).collect();
    assert_eq!(&listed[..4], &[b"Hero_Axe.Attack".as_slice(), b"type", b"dota_src1_3d", b"volume"]);
    assert_eq!(listed.iter().filter(|text| **text == b"sounds/a.vsnd").count(), 1);
    assert_eq!(listed.iter().filter(|text| **text == b"Hero_Axe.Attack.Impact").count(), 1, "a key and a string share their entry");
}

#[test]
fn values_of_another_version_or_compression_are_refused() {
    let mut block = write(&events(), &like()).unwrap();
    block[0] = 4;
    assert!(read(&block).is_err());
    let mut other = like();
    other[20] = 2;
    assert!(write(&events(), &other).is_err());
}

#[test]
fn values_cut_short_are_refused() {
    let block = write(&events(), &like()).unwrap();
    assert!(read(&block[..block.len() - 1]).is_err());
}

#[test]
fn lz4_with_a_match_unpacks() {
    let packed = [0x35, b'a', b'b', b'c', 3, 0, 0x50, b'x', b'y', b'z', b'z', b'y'];
    assert_eq!(lz4::unpack(&packed, 17).unwrap(), b"abcabcabcabcxyzzy");
    assert!(lz4::unpack(&packed, 18).is_err(), "not the size it says");
    assert!(lz4::unpack(&[0x35, b'a', b'b', b'c', 9, 0, 0x00], 12).is_err(), "a match before the start");
}

#[test]
fn bytes_packed_as_lz4_unpack_to_themselves() {
    for len in [0, 1, 14, 15, 16, 269, 270, 271, 600] {
        let bytes: Vec<u8> = (0..len).map(|n| (n * 7) as u8).collect();
        assert_eq!(lz4::unpack(&lz4::pack(&bytes), len).unwrap(), bytes, "{len} bytes");
    }
}

fn compiled_in(values: &Value, data_last: bool) -> Vec<u8> {
    let other = b"RED2 editing info".to_vec();
    let data = write(values, &like()).unwrap();
    let mut blocks = vec![(b"RED2", &other), (b"DATA", &data)];
    if !data_last {
        blocks.reverse();
    }
    let mut file = vec![0; 16 + 24];
    file[4..6].copy_from_slice(&12u16.to_le_bytes());
    file[6..8].copy_from_slice(&1u16.to_le_bytes());
    file[8..12].copy_from_slice(&8u32.to_le_bytes());
    file[12..16].copy_from_slice(&2u32.to_le_bytes());
    let mut at = 48;
    for (index, (name, block)) in blocks.into_iter().enumerate() {
        let entry = 16 + 12 * index;
        file[entry..entry + 4].copy_from_slice(name);
        file[entry + 4..entry + 8].copy_from_slice(&((at - entry - 4) as u32).to_le_bytes());
        file[entry + 8..entry + 12].copy_from_slice(&(block.len() as u32).to_le_bytes());
        file.resize(at, 0);
        file.extend_from_slice(block);
        at = file.len().div_ceil(16) * 16;
    }
    let len = file.len() as u32;
    file[0..4].copy_from_slice(&len.to_le_bytes());
    file
}

pub(crate) fn compiled(values: &Value) -> Vec<u8> {
    compiled_in(values, true)
}

#[test]
fn a_compiled_file_gets_other_values_and_keeps_the_rest() {
    let file = compiled(&events());
    let parsed = Compiled::parse(&file).unwrap();
    assert_eq!(parsed.values().unwrap(), events());
    let other = object(&[("Hero_Axe.Attack", object(&[("volume", Value::Double(1.0))]))]);
    let changed = parsed.with_values(&other).unwrap();
    assert_eq!(Compiled::parse(&changed).unwrap().values().unwrap(), other);
    assert_eq!(&changed[..parsed.data.start], &{
        let mut start = file[..parsed.data.start].to_vec();
        start[0..4].copy_from_slice(&(changed.len() as u32).to_le_bytes());
        let entry = parsed.entry;
        start[entry + 8..entry + 12].copy_from_slice(&((changed.len() - parsed.data.start) as u32).to_le_bytes());
        start
    }[..]);
    assert_eq!(u32_at(&changed, 0).unwrap() as usize, changed.len());
}

#[test]
fn a_compiled_file_whose_values_are_not_last_is_refused() {
    let file = compiled_in(&events(), false);
    assert!(Compiled::parse(&file).is_err());
}
