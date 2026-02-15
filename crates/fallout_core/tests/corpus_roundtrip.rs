#[test]
fn roundtrip_f1_corpus() {
    use std::fs;
    use std::io::Cursor;
    use fallout_core::fallout1::Document;

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let base = root.join("docs/test_fallout1_Saves/Fallout");

    let mut tested = 0;
    let mut failed = 0;

    for batch in &["Saves 1 to 10", "Saves 11 to 20", "Saves 21 to 30"] {
        let saves_dir = base.join(batch).join("data/SAVEGAME");
        if !saves_dir.exists() { continue; }

        let mut slots: Vec<_> = std::fs::read_dir(&saves_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        slots.sort_by_key(|e| e.file_name());

        for entry in slots {
            let save_path = entry.path().join("SAVE.DAT");
            if !save_path.exists() { continue; }

            let bytes = fs::read(&save_path).unwrap();
            let doc = match Document::parse_with_layout(Cursor::new(&bytes)) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("PARSE FAIL {}: {}", save_path.display(), e);
                    failed += 1;
                    tested += 1;
                    continue;
                }
            };
            let emitted = doc.to_bytes_unmodified().unwrap();
            if emitted != bytes {
                eprintln!("ROUNDTRIP FAIL {}", save_path.display());
                for (i, (a, b)) in bytes.iter().zip(emitted.iter()).enumerate() {
                    if a != b {
                        eprintln!("  first diff at byte {}: orig=0x{:02x} emit=0x{:02x}", i, a, b);
                        break;
                    }
                }
                if bytes.len() != emitted.len() {
                    eprintln!("  len orig={} emit={}", bytes.len(), emitted.len());
                }
                failed += 1;
            }
            tested += 1;
        }
    }

    eprintln!("Tested {} F1 corpus saves, {} failed", tested, failed);
    assert_eq!(failed, 0, "some roundtrips failed");
}

#[test]
fn roundtrip_f2_corpus() {
    use std::fs;
    use std::io::Cursor;
    use fallout_core::fallout2::Document;

    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let base = root.join("docs/test_fallout2_saves/Fallout 2");

    let mut tested = 0;
    let mut failed = 0;

    for batch in &[
        "Saves 1 to 10", "Saves 11 to 20", "Saves 21 to 30",
        "Saves 31 to 40", "Saves 41 to 50", "Saves 51 to 60",
    ] {
        let saves_dir = base.join(batch).join("data/SAVEGAME");
        if !saves_dir.exists() { continue; }

        let mut slots: Vec<_> = std::fs::read_dir(&saves_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        slots.sort_by_key(|e| e.file_name());

        for entry in slots {
            let save_path = entry.path().join("SAVE.DAT");
            if !save_path.exists() { continue; }

            let bytes = fs::read(&save_path).unwrap();
            let doc = match Document::parse_with_layout(Cursor::new(&bytes)) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("PARSE FAIL {}: {}", save_path.display(), e);
                    failed += 1;
                    tested += 1;
                    continue;
                }
            };
            let emitted = doc.to_bytes_unmodified().unwrap();
            if emitted != bytes {
                eprintln!("ROUNDTRIP FAIL {}", save_path.display());
                for (i, (a, b)) in bytes.iter().zip(emitted.iter()).enumerate() {
                    if a != b {
                        eprintln!("  first diff at byte {}: orig=0x{:02x} emit=0x{:02x}", i, a, b);
                        break;
                    }
                }
                if bytes.len() != emitted.len() {
                    eprintln!("  len orig={} emit={}", bytes.len(), emitted.len());
                }
                failed += 1;
            }
            tested += 1;
        }
    }

    eprintln!("Tested {} F2 corpus saves, {} failed", tested, failed);
    assert_eq!(failed, 0, "some roundtrips failed");
}
