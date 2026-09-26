//! End-to-end checks for the versioned compiler-native Foundry linking protocol.

use super::*;

#[test]
#[ignore = "requires Foundry with the native Solar test-link protocol on PATH"]
fn native_test_linking_cache() {
    assert!(forge_available(), "forge is required");
    let project = tempfile::tempdir().unwrap();
    let fixture = foundry_root().join("test-linking");
    for path in ["foundry.toml", "src/Child.sol", "test/Linking.t.sol", "test/Reuse.t.sol"] {
        let destination = project.path().join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(fixture.join(path), destination).unwrap();
    }
    let solar = get_solar_binary();
    let run = || {
        Command::new("forge")
            .current_dir(project.path())
            .env("SOLC_WRAPPER", "1")
            .env("SOLC_WRAPPER_VERSION", "0.8.30")
            .args(["test", "--json", "--threads", "1", "--use"])
            .arg(&solar)
            .output()
            .unwrap()
    };
    let output = run();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let tests = parse_test_results(&String::from_utf8_lossy(&output.stdout));
    assert_eq!(tests.len(), 9);
    assert!(tests.iter().all(|test| test.passed));

    let dynamic = project.path().join("out/Reuse.t.sol/ReuseTest.json");
    let native = project.path().join("out/Linking.t.sol/LinkingTest.json");
    let dynamic_before = fs::read(&dynamic).unwrap();
    let native_before = fs::read(&native).unwrap();
    let artifact: serde_json::Value = serde_json::from_slice(&dynamic_before).unwrap();
    let metadata: serde_json::Value =
        serde_json::from_str(artifact["rawMetadata"].as_str().unwrap()).unwrap();
    assert!(
        !metadata["settings"]["solarTestLinks"].as_array().unwrap().is_empty(),
        "source preprocessing must not substitute for native linking in this test"
    );

    // This body-only edit must reach the runtime without regenerating the purely dynamic test.
    // The second test file also contains try/new, so it must be rebuilt as a native dependent.
    let child = project.path().join("src/Child.sol");
    let source = fs::read_to_string(&child).unwrap();
    let edited = source.replace("number = config.number;", "number = config.number + 1;");
    assert_ne!(source, edited);
    fs::write(&child, edited).unwrap();
    let output = run();
    assert!(!output.status.success());
    let tests = parse_test_results(&String::from_utf8_lossy(&output.stdout));
    let reused = tests.iter().find(|test| test.contract == "ReuseTest").unwrap();
    assert!(!reused.passed, "the cached test must deploy the edited constructor");
    assert_eq!(fs::read(dynamic).unwrap(), dynamic_before);
    assert_ne!(fs::read(native).unwrap(), native_before);
}
