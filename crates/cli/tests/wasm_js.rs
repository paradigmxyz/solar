#![allow(unused_crate_dependencies)]

use std::{path::Path, process::Command};

#[test]
fn js_wrapper_api_shape_and_compile_behavior() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipping JS wrapper test because node is not available");
        return;
    }

    let wrapper = Path::new(env!("CARGO_MANIFEST_DIR")).join("soljson.js");
    let wrapper = serde_json::to_string(&wrapper).unwrap();
    let script = format!(
        r#"
const assert = require("node:assert/strict");
const solar = require({wrapper});

let callbackPath = null;
const compiler = solar.setupMethods({{
  version() {{
    return "0.1.8+commit.test";
  }},
  semver() {{
    return "0.1.8";
  }},
  license() {{
    return "MIT OR Apache-2.0";
  }},
  compileStandard(input, callbacks) {{
    const imported = callbacks.import("B.sol");
    callbackPath = "B.sol";
    return JSON.stringify({{
      language: JSON.parse(input).language,
      imported,
    }});
  }},
}});

assert.equal(typeof compiler.compile, "function");
assert.equal(compiler.version(), "0.1.8+commit.test");
assert.equal(compiler.semver(), "0.1.8");
assert.equal(compiler.license(), "MIT OR Apache-2.0");
assert.equal(compiler.features.nativeStandardJSON, true);
assert.equal(compiler.features.importCallback, true);
assert.equal(compiler.lowlevel.compileSingle, null);
assert.equal(compiler.lowlevel.compileMulti, null);
assert.equal(compiler.lowlevel.compileCallback, null);
assert.equal(typeof compiler.lowlevel.compileStandard, "function");

const output = JSON.parse(compiler.compile(
  JSON.stringify({{ language: "Solidity" }}),
  {{ import(path) {{ return {{ contents: `contract B {{}} // ${{path}}` }}; }} }}
));
assert.equal(output.language, "Solidity");
assert.deepEqual(output.imported, {{ contents: "contract B {{}} // B.sol" }});
assert.equal(callbackPath, "B.sol");
"#
    );

    let output = Command::new("node").arg("-e").arg(script).output().unwrap();
    assert!(
        output.status.success(),
        "node failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
