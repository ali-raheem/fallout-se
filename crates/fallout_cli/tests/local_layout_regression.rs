#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

struct ExpectedSection {
    id: &'static str,
    start: u64,
    end: u64,
}

fn save_dir() -> PathBuf {
    PathBuf::from(r"C:\FalloutRE\saves")
}

fn sha256(path: &Path) -> String {
    let output = Command::new("certutil")
        .arg("-hashfile")
        .arg(path)
        .arg("SHA256")
        .output()
        .expect("failed to execute certutil");

    assert!(
        output.status.success(),
        "certutil failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    stdout
        .lines()
        .map(|line| line.trim())
        .find(|line| line.len() == 64 && line.bytes().all(|b| b.is_ascii_hexdigit()))
        .expect("SHA256 digest not found in certutil output")
        .to_ascii_uppercase()
}

fn section<'a>(json: &'a Value, id: &str) -> &'a Value {
    let sections = json["sections"]
        .as_array()
        .expect("sections must be an array");

    let matches: Vec<_> = sections
        .iter()
        .filter(|entry| entry["id"].as_str() == Some(id))
        .collect();

    assert_eq!(matches.len(), 1, "expected exactly one section named {id}");

    matches[0]
}

fn check_save(filename: &str, expected_hash: &str, expected_sections: &[ExpectedSection]) {
    let path = save_dir().join(filename);

    if !path.exists() {
        eprintln!("SKIP: local fixture missing: {}", path.display());
        return;
    }

    assert!(
        path.is_file(),
        "fixture path is not a regular file: {}",
        path.display()
    );

    assert_eq!(
        sha256(&path),
        expected_hash,
        "fixture SHA256 mismatch: {}",
        path.display()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_fallout-se"))
        .args(["debug", "layout", "--json"])
        .arg(&path)
        .output()
        .expect("failed to execute fallout-se");

    assert!(
        output.status.success(),
        "layout command failed for {}:\n{}",
        filename,
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value =
        serde_json::from_slice(&output.stdout).expect("layout output must be valid JSON");

    assert_eq!(json["game"], "Fallout2");
    assert_eq!(json["validation_ok"], true);

    for expected in expected_sections {
        let actual = section(&json, expected.id);

        assert_eq!(
            actual["start"].as_u64(),
            Some(expected.start),
            "{}: {} start",
            filename,
            expected.id
        );

        assert_eq!(
            actual["end"].as_u64(),
            Some(expected.end),
            "{}: {} end",
            filename,
            expected.id
        );

        assert_eq!(
            actual["len"].as_u64(),
            Some(expected.end - expected.start),
            "{}: {} length",
            filename,
            expected.id
        );
    }
}

#[test]
fn old_layout_regression() {
    check_save(
        "OLD.DAT",
        "31808E2E6D6082B2674911581BBD51AECD8C516AAE7D9795ECFAA587EE1C2A26",
        &[
            ExpectedSection {
                id: "handler:10",
                start: 36344,
                end: 48720,
            },
            ExpectedSection {
                id: "handler:11",
                start: 48720,
                end: 48724,
            },
            ExpectedSection {
                id: "handler:12",
                start: 48724,
                end: 52144,
            },
            ExpectedSection {
                id: "handler:13",
                start: 52144,
                end: 52164,
            },
        ],
    );
}

#[test]
fn rpu_layout_regression() {
    check_save(
        "RPU.DAT",
        "1CBD19AF06539A66E3C29B5CB174F2D6E4141D8AFF9614F483AFCC82EF521785",
        &[
            ExpectedSection {
                id: "handler:10",
                start: 37104,
                end: 50908,
            },
            ExpectedSection {
                id: "handler:11",
                start: 50908,
                end: 50912,
            },
            ExpectedSection {
                id: "handler:12",
                start: 50912,
                end: 54872,
            },
            ExpectedSection {
                id: "handler:13",
                start: 54872,
                end: 54892,
            },
        ],
    );
}

#[test]
fn rpu_test_layout_regression() {
    check_save(
        "RPU-TEST.DAT",
        "3D70473F0A9FD3CF306D13BFF36F02CDCC24FC6BA0E6FD571E8DC89E886272B8",
        &[
            ExpectedSection {
                id: "handler:10",
                start: 37104,
                end: 50908,
            },
            ExpectedSection {
                id: "handler:11",
                start: 50908,
                end: 50912,
            },
            ExpectedSection {
                id: "handler:12",
                start: 50912,
                end: 54872,
            },
            ExpectedSection {
                id: "handler:13",
                start: 54872,
                end: 54892,
            },
        ],
    );
}
