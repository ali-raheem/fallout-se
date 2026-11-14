use std::process::Command;
fn arguments() -> Vec<&'static str> {
    vec![
        "experimental-rpu-edit-quantity",
        "--save",
        "C:/missing/SAVE.DAT",
        "--game-dir",
        "C:/missing/game",
        "--source-plan",
        "C:/missing/plan.json",
        "--object-path",
        "1",
        "--expected-pid",
        "7",
        "--expected-object-id",
        "11",
        "--expected-quantity",
        "1",
        "--new-quantity",
        "10001",
        "--output",
        "C:/missing/output.dat",
        "--experimental-consent",
    ]
}
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fallout-se"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn experimental_help_lists_required_safety_arguments() {
    let result = run(&["experimental-rpu-edit-quantity", "--help"]);
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    for flag in [
        "--source-plan",
        "--experimental-consent",
        "--dry-run",
        "--expected-object-id",
        "--object-path",
    ] {
        assert!(text.contains(flag));
    }
}
#[test]
fn missing_consent_rejected_before_filesystem_access() {
    let mut args = arguments();
    args.pop();
    let result = run(&args);
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--experimental-consent"));
}
#[test]
fn exact_identity_arguments_are_required() {
    for flag in [
        "--save",
        "--game-dir",
        "--source-plan",
        "--object-path",
        "--expected-pid",
        "--expected-object-id",
        "--expected-quantity",
        "--new-quantity",
        "--output",
    ] {
        let mut args = arguments();
        let index = args.iter().position(|s| *s == flag).unwrap();
        args.drain(index..index + 2);
        let result = run(&args);
        assert_eq!(result.status.code(), Some(2), "{flag}");
    }
}
#[test]
fn malformed_path_and_overflow_quantity_are_rejected() {
    for (flag, value) in [
        ("--object-path", "1..2"),
        ("--object-path", "-1"),
        ("--object-path", "x"),
        ("--new-quantity", "2147483648"),
    ] {
        let mut args = arguments();
        let index = args.iter().position(|s| *s == flag).unwrap() + 1;
        args[index] = value;
        assert_eq!(run(&args).status.code(), Some(2));
    }
}
#[test]
fn missing_resource_plan_fails_without_output() {
    let result = run(&arguments());
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("plan.json"));
}

#[test]
fn nested_object_path_dry_run_and_signed_quantities_parse() {
    let mut args = arguments();
    let i = args.iter().position(|s| *s == "--object-path").unwrap() + 1;
    args[i] = "0.2";
    let i = args.iter().position(|s| *s == "--new-quantity").unwrap() + 1;
    args[i] = "-1";
    args.push("--dry-run");
    // Parsing succeeds; the explicitly missing plan then fails before any write.
    let result = run(&args);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stderr).contains("plan.json"));
}

#[cfg(windows)]
#[test]
#[ignore = "explicit coherent installed RPU CLI export; fresh temporary output only"]
fn installed_coherent_rpu_cli_retains_and_verifies_candidate() {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    struct OwnedTemp {
        root: PathBuf,
        expected: PathBuf,
    }
    impl Drop for OwnedTemp {
        fn drop(&mut self) {
            let actual = self.root.canonicalize().unwrap();
            assert_eq!(actual, self.expected);
            assert_eq!(
                actual.parent(),
                Some(std::env::temp_dir().canonicalize().unwrap().as_path())
            );
            assert!(
                actual
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("rpu-cli-test-")
            );
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    let root = std::env::temp_dir().join(format!(
        "rpu-cli-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let temp = OwnedTemp {
        expected: root.canonicalize().unwrap(),
        root,
    };
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let save = project.join("tests/fallout2_examples/SLOT01/SAVE.DAT");
    let original = fs::read(&save).unwrap();
    let output = temp.root.join("candidate.dat");
    let mut args: Vec<String> = arguments().iter().map(|s| s.to_string()).collect();
    for (flag, value) in [
        ("--save", save.to_str().unwrap()),
        ("--game-dir", "C:/Fallout-2-mod"),
        (
            "--source-plan",
            project
                .join("rpu-offline-source-plan.json")
                .to_str()
                .unwrap(),
        ),
        ("--output", output.to_str().unwrap()),
    ] {
        let i = args.iter().position(|s| s == flag).unwrap() + 1;
        args[i] = value.into();
    }
    let dry = Command::new(env!("CARGO_BIN_EXE_fallout-se"))
        .args(&args)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert!(!output.exists());
    let edited = Command::new(env!("CARGO_BIN_EXE_fallout-se"))
        .args(&args)
        .output()
        .unwrap();
    assert!(
        edited.status.success(),
        "{}",
        String::from_utf8_lossy(&edited.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&edited.stdout).unwrap();
    assert_eq!(
        report["edited_sha256"],
        "d8e064a74970736939d3ec1fad31b5df13c7b139eac127a2b16f5313d2bf91e0"
    );
    assert_eq!(report["handler6"], 36744);
    assert_eq!(report["publication"], "Published");
    let mut expected = original.clone();
    expected[36640..36644].copy_from_slice(&10001i32.to_be_bytes());
    assert_eq!(fs::read(&output).unwrap(), expected);
    assert_eq!(fs::read(&save).unwrap(), original);
    let repeat = Command::new(env!("CARGO_BIN_EXE_fallout-se"))
        .args(&args)
        .output()
        .unwrap();
    assert!(!repeat.status.success());
    assert_eq!(fs::read(&output).unwrap(), expected);
}
