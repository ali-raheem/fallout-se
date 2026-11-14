//! Characterizations of the CURRENT heuristic parser and rewriting setters.
//! Passing these tests does not mean inventory parsing has been fixed.
//! Offsets below are fixture wire-format oracles, not parser-discovered ranges.
use std::io::Cursor;

use fallout_core::core_api::{Engine, Game};
use fallout_core::object::{GameObject, ObjectData};
use fallout_core::reader::BigEndianReader;

const F2: &[u8] = include_bytes!("../../../tests/fallout2_examples/SLOT01/SAVE.DAT");
const F1: &[u8] = include_bytes!("../../../tests/fallout1_examples/SAVEGAME/SLOT01/SAVE.DAT");
const H5_START: usize = 36_416;
const H5_END: usize = 36_736;
const FIRST_QUANTITY: usize = 36_544;
const FIRST_OBJECT: usize = 36_548;
const SECOND_QUANTITY: usize = 36_640;
const SECOND_OBJECT: usize = 36_644;
const QUANTITIES: [i32; 5] = [9_999, 10_000, 10_001, 10_002, i32::MAX];

fn read(bytes: &[u8], offset: usize) -> i32 {
    i32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn put(bytes: &mut [u8], offset: usize, value: i32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn open(bytes: &[u8]) -> fallout_core::core_api::Session {
    Engine::new()
        .open_bytes(bytes, Some(Game::Fallout2))
        .unwrap()
}

fn differences(before: &[u8], after: &[u8]) -> Vec<usize> {
    assert_eq!(before.len(), after.len());
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter_map(|(i, (a, b))| (a != b).then_some(i))
        .collect()
}

fn current_rewrite_oracle(quantity: i32) -> Vec<u8> {
    let mut expected = F2.to_vec();
    put(&mut expected, SECOND_QUANTITY, quantity);
    // Known unrelated mutation by the existing full-object emitter.
    put(&mut expected, H5_START + 80, 0);
    expected
}

#[test]
fn slot01_wire_oracle_identifies_quantities_pids_ids_and_payload_boundaries() {
    assert_eq!(read(F2, H5_START + 44), 0x0100_0000);
    assert_eq!(read(F2, H5_START + 72), 2);
    assert_eq!(read(F2, H5_START + 76), 10);
    assert_eq!(read(F2, H5_START + 80), 253_250_888);
    assert_eq!(H5_START + 84 + 44, FIRST_QUANTITY); // Critter header + update data.
    assert_eq!(read(F2, FIRST_QUANTITY), 2);
    assert_eq!(read(F2, FIRST_OBJECT), 15);
    assert_eq!(read(F2, FIRST_OBJECT + 44), 591);
    assert_eq!(read(F2, FIRST_OBJECT + 72), 0);
    assert_eq!(&F2[FIRST_OBJECT + 84..SECOND_QUANTITY], &[0; 8]); // Flags + 4-byte payload.
    assert_eq!(FIRST_OBJECT + 84 + 4 + 4, SECOND_QUANTITY);
    assert_eq!(read(F2, SECOND_QUANTITY), 1);
    assert_eq!(read(F2, SECOND_OBJECT), 11);
    assert_eq!(read(F2, SECOND_OBJECT + 44), 7);
    assert_eq!(read(F2, SECOND_OBJECT + 72), 0);
    assert_eq!(read(F2, SECOND_OBJECT + 84), 0); // Flags, no extra payload.
    assert_eq!(SECOND_OBJECT + 88 + 4, H5_END); // Object + center tile.
}

#[test]
fn quantity_threshold_matrix_characterizes_semantic_reinterpretation() {
    for quantity in QUANTITIES {
        let mut session = open(F2);
        session.set_inventory_quantity(7, quantity).unwrap();
        let bytes = session.to_bytes_modified().unwrap();
        let expected = current_rewrite_oracle(quantity);
        assert!(
            bytes == expected,
            "initial quantity edit changed unrelated bytes: {quantity}; differences={:?}",
            differences(&expected, &bytes)
        );
        assert_eq!(read(&bytes, SECOND_OBJECT), 11);
        assert_eq!(read(&bytes, SECOND_OBJECT + 44), 7);
        let reopened = open(&bytes);
        let item = &reopened.inventory()[1];
        if quantity <= 10_000 {
            assert_eq!((item.pid, item.quantity), (7, quantity));
        } else {
            // Known bug: previous item absorbs this quantity into its payload.
            assert_eq!((item.pid, item.quantity), (-1, 11));
            assert_ne!(reopened.inventory(), session.inventory());
        }
        assert!(session.to_bytes_unmodified().unwrap() == F2);
    }
}

#[test]
fn editing_misread_quantity_overwrites_the_original_object_id() {
    let mut session = open(F2);
    session.set_inventory_quantity(7, 10_001).unwrap();
    let before = session.to_bytes_modified().unwrap();
    let mut reopened = open(&before);
    assert_eq!(reopened.inventory()[1].pid, -1);
    assert_eq!(reopened.inventory()[1].quantity, 11);
    reopened.set_inventory_quantity(-1, 12).unwrap();
    let after = reopened.to_bytes_modified().unwrap();
    // Characterize secondary corruption; this is deliberately NOT a safety assertion.
    assert_eq!(differences(&before, &after), vec![SECOND_OBJECT + 3]);
    assert_eq!(read(&after, SECOND_QUANTITY), 10_001);
    assert_eq!(read(&after, SECOND_OBJECT), 12);
    assert_eq!(read(&after, SECOND_OBJECT + 44), 7);
}

#[test]
fn sequential_updates_characterize_quantity_changes_and_pointer_loss() {
    let mut session = open(F2);
    for quantity in QUANTITIES.into_iter().chain([1, 10_000]) {
        session.set_inventory_quantity(7, quantity).unwrap();
        let expected = current_rewrite_oracle(quantity);
        assert!(session.to_bytes_modified().unwrap() == expected);
        assert!(session.to_bytes_unmodified().unwrap() == F2);
    }
}

#[test]
fn repeated_reopening_can_preserve_bytes_while_retaining_wrong_semantics() {
    for quantity in QUANTITIES {
        let mut bytes = current_rewrite_oracle(quantity);
        for _ in 0..3 {
            let mut session = open(&bytes);
            let expected = if quantity <= 10_000 {
                (7, quantity)
            } else {
                (-1, 11)
            };
            let item = &session.inventory()[1];
            assert_eq!((item.pid, item.quantity), expected);
            assert!(session.to_bytes_modified().unwrap() == bytes);
            // Even forcing a model rewrite does not prove the model is correct.
            session.set_inventory_quantity(591, 2).unwrap();
            assert!(session.to_bytes_modified().unwrap() == bytes);
            bytes = session.to_bytes_modified().unwrap();
        }
    }
    for (game, fixture) in [(Game::Fallout1, F1), (Game::Fallout2, F2)] {
        let mut bytes = fixture.to_vec();
        for _ in 0..3 {
            let session = Engine::new().open_bytes(&bytes, Some(game)).unwrap();
            assert!(session.to_bytes_unmodified().unwrap() == fixture);
            bytes = session.to_bytes_modified().unwrap();
            assert!(bytes == fixture);
        }
    }
}

// Independent wire builder. It does not call the production emitter or parser.
// Subtypes/payload lengths are explicit test input, not inferred from the PID.
#[derive(Clone)]
struct WireObject {
    id: i32,
    pid: i32,
    data: Vec<i32>,
    children: Vec<(i32, WireObject)>,
    capacity: i32,
    pointer: i32,
}

fn item(id: i32, pid: i32) -> WireObject {
    WireObject {
        id,
        pid,
        data: vec![0],
        children: vec![],
        capacity: 0,
        pointer: 0,
    }
}

fn wire(object: &WireObject) -> Vec<u8> {
    let mut words = vec![0i32; 21];
    words[0] = object.id;
    words[1] = -1;
    words[11] = object.pid;
    words[12] = -1;
    words[16] = -1;
    words[17] = -1;
    words[18] = object.children.len().try_into().unwrap();
    words[19] = object.capacity;
    words[20] = object.pointer;
    words.extend(&object.data);
    let mut bytes: Vec<_> = words.into_iter().flat_map(i32::to_be_bytes).collect();
    for (quantity, child) in &object.children {
        bytes.extend(quantity.to_be_bytes());
        bytes.extend(wire(child));
    }
    bytes
}

fn fixture_with_items(items: &[(i32, WireObject)]) -> Vec<u8> {
    let mut handler = F2[H5_START..FIRST_QUANTITY].to_vec();
    put(&mut handler, 72, items.len().try_into().unwrap());
    put(&mut handler, 76, items.len().try_into().unwrap());
    put(&mut handler, 80, 0);
    for (quantity, object) in items {
        handler.extend(quantity.to_be_bytes());
        handler.extend(wire(object));
    }
    handler.extend(&F2[H5_END - 4..H5_END]);
    let mut bytes = F2[..H5_START].to_vec();
    bytes.extend(handler);
    bytes.extend(&F2[H5_END..]);
    bytes
}

fn parse_object(bytes: &[u8]) -> (GameObject, usize) {
    let mut reader = BigEndianReader::new(Cursor::new(bytes));
    let object = GameObject::parse(&mut reader).unwrap();
    (object, reader.position().unwrap().try_into().unwrap())
}

#[test]
fn first_middle_last_unknown_items_have_independent_quantity_and_identity_oracles() {
    // IDs above 10000 keep these zero-extra-byte control objects from receiving
    // spurious positive lookahead scores. They do not fix the heuristic parser.
    let items: Vec<_> = (0..3)
        .map(|i| (1, item(20_001 + i, 0x00F0_0101 + i)))
        .collect();
    let original = fixture_with_items(&items);
    for index in 0..3 {
        let offset = FIRST_QUANTITY + index * (4 + 88);
        assert_eq!(read(&original, offset), 1);
        assert_eq!(read(&original, offset + 4), items[index].1.id);
        assert_eq!(read(&original, offset + 4 + 44), items[index].1.pid);
        for quantity in QUANTITIES {
            let mut session = open(&original);
            session
                .set_inventory_quantity(items[index].1.pid, quantity)
                .unwrap();
            let bytes = session.to_bytes_modified().unwrap();
            let mut expected = original.clone();
            put(&mut expected, offset, quantity);
            assert!(bytes == expected);
            assert_eq!(open(&bytes).inventory(), session.inventory());
        }
    }
}

#[test]
fn duplicate_pid_setter_collapses_distinct_objects_even_for_unchanged_quantity() {
    let pid = 0x00F0_0101;
    let original = fixture_with_items(&[(1, item(20_001, pid)), (1, item(20_002, pid))]);
    assert_eq!(read(&original, FIRST_QUANTITY + 4), 20_001);
    assert_eq!(read(&original, FIRST_QUANTITY + 92 + 4), 20_002);
    let mut session = open(&original);
    assert_eq!(session.inventory().len(), 2);
    session.set_inventory_quantity(pid, 1).unwrap();
    let bytes = session.to_bytes_modified().unwrap();
    // Known PID-based API behavior: duplicate objects are discarded, not patched.
    assert_eq!(session.inventory().len(), 1);
    assert_eq!(bytes.len(), original.len() - 92);
    assert_eq!(read(&bytes, FIRST_QUANTITY + 4), 20_001);
    assert_eq!(read(&bytes, H5_START + 72), 1);
    assert!(session.to_bytes_unmodified().unwrap() == original);
}

#[test]
fn nonzero_four_and_eight_byte_payload_controls_survive_first_quantity_edits() {
    let mut first = item(20_001, 0x00F0_0301);
    first.data = vec![0x1122_3344, 0x5566_7788]; // Flags + four opaque payload bytes.
    let mut second = item(20_002, 0x00F0_0302);
    second.data = vec![0x1234_5678, -123_456_789, 0x1122_3344]; // Flags + eight bytes.
    let original = fixture_with_items(&[(1, first), (1, second), (1, item(20_003, 0x00F0_0303))]);
    let first_payload = FIRST_QUANTITY + 4 + 88;
    let last_quantity = FIRST_QUANTITY + 4 + 92;
    let last_payload = last_quantity + 4 + 88;
    assert_eq!(read(&original, first_payload), 0x5566_7788);
    assert_eq!(read(&original, last_payload), -123_456_789);
    assert_eq!(read(&original, last_payload + 4), 0x1122_3344);
    let doc = fallout_core::fallout2::Document::parse_with_layout(Cursor::new(&original)).unwrap();
    for (index, expected_extra) in [(0, 4), (1, 8), (2, 0)] {
        let ObjectData::Item(data) = &doc.save.player_object.inventory[index].object.object_data
        else {
            panic!("expected item")
        };
        assert_eq!(data.extra_bytes, expected_extra);
    }
    for quantity in QUANTITIES {
        let mut session = open(&original);
        session
            .set_inventory_quantity(0x00F0_0301, quantity)
            .unwrap();
        let mut expected = original.clone();
        put(&mut expected, FIRST_QUANTITY, quantity);
        let bytes = session.to_bytes_modified().unwrap();
        assert!(bytes == expected);
        assert_eq!(open(&bytes).inventory(), session.inventory());
    }
}

#[test]
fn final_item_payload_can_be_captured_as_the_following_section_without_byte_loss() {
    let mut last = item(20_001, 0x00F0_0401);
    last.data = vec![0, -123_456_789, 0x1122_3344];
    let original = fixture_with_items(&[(1, last)]);
    let payload_start = FIRST_QUANTITY + 4 + 88;
    let true_h5_end = payload_start + 8 + 4;
    let doc = fallout_core::fallout2::Document::parse_with_layout(Cursor::new(&original)).unwrap();
    let ObjectData::Item(data) = &doc.save.player_object.inventory[0].object.object_data else {
        panic!("expected item")
    };
    // No sibling supplies a plausible quantity, so the parser selects zero extra
    // bytes. Handler 6's nearby-anchor search absorbs the remainder instead.
    assert_eq!(data.extra_bytes, 0);
    assert_eq!(doc.save.center_tile, -123_456_789);
    let h5 = doc
        .layout()
        .sections
        .iter()
        .find(|s| s.id == fallout_core::layout::SectionId::Handler(5))
        .unwrap();
    assert_eq!(h5.range.end, true_h5_end - 8);
    assert!(doc.to_bytes_modified().unwrap() == original);
}

#[test]
fn nested_container_controls_preserve_child_ids_payload_and_parent_siblings() {
    let mut container = item(20_001, 0x00F0_0201);
    container.capacity = 1;
    container.children.push((1, item(30_001, 0x00F0_0202)));
    let original = fixture_with_items(&[(1, container), (1, item(20_002, 0x00F0_0203))]);
    let child_quantity = FIRST_QUANTITY + 4 + 88;
    let sibling_quantity = child_quantity + 4 + 88;
    assert_eq!(read(&original, child_quantity + 4), 30_001);
    assert_eq!(read(&original, sibling_quantity + 4), 20_002);
    for quantity in QUANTITIES {
        let mut bytes = original.clone();
        put(&mut bytes, child_quantity, quantity);
        let doc = fallout_core::fallout2::Document::parse_with_layout(Cursor::new(&bytes)).unwrap();
        assert_eq!(
            doc.save.player_object.inventory[0].object.inventory[0].quantity,
            quantity
        );
        assert_eq!(
            doc.save.player_object.inventory[0].object.inventory[0]
                .object
                .id,
            30_001
        );
        assert_eq!(doc.save.player_object.inventory[1].object.id, 20_002);
        assert!(doc.to_bytes_modified().unwrap() == bytes);
        let mut session = open(&bytes);
        // Root quantity edit rewrites nested objects as well in the existing API.
        session.set_inventory_quantity(0x00F0_0203, 2).unwrap();
        let mut expected = bytes.clone();
        put(&mut expected, sibling_quantity, 2);
        assert!(session.to_bytes_modified().unwrap() == expected);
    }
}

#[test]
fn missing_subtype_metadata_allows_truncated_weapon_to_be_reinterpreted() {
    let mut weapon = item(20_001, 7);
    weapon.data = vec![0, 20_001, -1]; // Flags + two weapon words, 96 bytes total.
    let complete = wire(&weapon);
    assert_eq!(complete.len(), 96);
    for cut in [0, 71, 83, 87] {
        let mut reader = BigEndianReader::new(Cursor::new(&complete[..cut]));
        assert_eq!(
            GameObject::parse(&mut reader).unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }
    let truncated = &complete[..92]; // Second weapon word is missing.
    let (parsed, consumed) = parse_object(truncated);
    let ObjectData::Item(data) = parsed.object_data else {
        panic!("expected item")
    };
    // Known bug: metadata-free parser accepts a different payload class.
    assert_eq!(data.extra_bytes, 4);
    assert_eq!(data.extra_data, 20_001i32.to_be_bytes());
    assert_eq!(consumed, truncated.len());
}

#[test]
fn unknown_object_category_is_currently_silently_treated_as_scenery() {
    let original = wire(&item(20_001, 0x0600_0001));
    let (parsed, consumed) = parse_object(&original);
    assert!(matches!(parsed.object_data, ObjectData::Scenery(_)));
    assert_eq!(consumed, original.len());
    assert!(parsed.emit_bytes().unwrap() == original);
}

#[test]
fn pointer_and_capacity_normalization_occurs_only_after_a_model_rewrite() {
    let mut original = fixture_with_items(&[(1, item(20_001, 0x00F0_0101))]);
    put(&mut original, H5_START + 76, -7);
    put(&mut original, H5_START + 80, 0x1234_5678);
    let mut session = open(&original);
    assert!(session.to_bytes_modified().unwrap() == original);
    session.set_inventory_quantity(0x00F0_0101, 2).unwrap();
    let bytes = session.to_bytes_modified().unwrap();
    let mut expected = original.clone();
    put(&mut expected, FIRST_QUANTITY, 2);
    put(&mut expected, H5_START + 76, 1);
    put(&mut expected, H5_START + 80, 0);
    // Characterize existing loss of opaque/header data, not desired patch behavior.
    assert!(bytes == expected);
    assert!(session.to_bytes_unmodified().unwrap() == original);

    let mut roomy = original.clone();
    put(&mut roomy, H5_START + 76, 12_345);
    put(&mut roomy, H5_START + 80, 0);
    let mut session = open(&roomy);
    session.set_inventory_quantity(0x00F0_0101, 2).unwrap();
    let mut expected = roomy;
    put(&mut expected, FIRST_QUANTITY, 2);
    assert!(session.to_bytes_modified().unwrap() == expected);
}
