//! Final MIR captures preserve bytecode while exposing resolved backend memory.

use serde_json::Value;
use std::process::Command;

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn compile(source: &str, mode: &str, dump: Option<&str>) -> Value {
    let mut command = Command::new(SOLAR);
    command.current_dir(ROOT).args([
        source,
        "--allow=2264",
        "-j1",
        "-O",
        mode,
        "--emit=bin,bin-runtime",
    ]);
    if let Some(dump) = dump {
        command.arg(dump);
    }
    let output = command.output().expect("run compiler");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    text.char_indices()
        .filter(|&(_, c)| c == '{')
        .find_map(|(index, _)| {
            serde_json::from_str::<Value>(&text[index..])
                .ok()
                .filter(|json| json.get("contracts").is_some())
        })
        .expect("compiler contract JSON")
}

#[test]
fn final_mir_capture_preserves_bytecode() {
    for source in [
        "tests/ui/codegen/mir/interp/backend_memory.mir",
        "tests/ui/codegen/mir/interp/tail_calls.mir",
        "tests/ui/codegen/lowering/run-call/heap_floor_constructor.sol",
    ] {
        for mode in ["none", "gas", "size"] {
            let baseline = compile(source, mode, None);
            for dump in ["-Zdump=mir-final", "-Zdump=mir,mir-final"] {
                let captured = compile(source, mode, Some(dump));
                assert_eq!(
                    baseline["contracts"], captured["contracts"],
                    "{source}: {mode}, {dump}"
                );
            }
        }
    }
}
