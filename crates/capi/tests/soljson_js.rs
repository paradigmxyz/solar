#![allow(unused_crate_dependencies)]

use std::{path::Path, process::Command};

fn js_string(path: &Path) -> String {
    format!("{:?}", path.to_string_lossy())
}

#[test]
fn js_wrapper_api_shape_and_compile_behavior() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipping JS wrapper test because node is not available");
        return;
    }

    let wrapper = Path::new(env!("CARGO_MANIFEST_DIR")).join("soljson.js");
    let wrapper = js_string(&wrapper);
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

#[test]
fn js_wrapper_c_abi_callbacks_match_solcjs_shape() {
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipping JS wrapper test because node is not available");
        return;
    }

    let wrapper = Path::new(env!("CARGO_MANIFEST_DIR")).join("soljson.js");
    let wrapper = js_string(&wrapper);
    let script = r#"
const assert = require("node:assert/strict");
const solar = require(__WRAPPER__);

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8");
const memory = new WebAssembly.Memory({ initial: 1 });
const bytes = new Uint8Array(memory.buffer);
const callbacks = new Map();
const freed = [];
let nextPtr = 8;
let resetCount = 0;

function alloc(size) {
  const ptr = nextPtr;
  nextPtr += size;
  return ptr;
}

function free(ptr) {
  freed.push(ptr);
}

function writeString(value) {
  const data = encoder.encode(String(value) + "\0");
  const ptr = alloc(data.length);
  bytes.set(data, ptr);
  return ptr;
}

function readString(ptr) {
  let end = ptr;
  while (bytes[end] !== 0) {
    end++;
  }
  return decoder.decode(bytes.subarray(ptr, end));
}

function setPointer(ptr, value) {
  new DataView(memory.buffer).setUint32(ptr, value, true);
}

function getPointer(ptr) {
  return new DataView(memory.buffer).getUint32(ptr, true);
}

const soljson = {
  memory,
  solidity_alloc: alloc,
  solidity_free: free,
  solidity_reset() {
    resetCount++;
  },
  solidity_compile(inputPtr, callbackPtr, context) {
    assert.equal(context, 0);
    const input = JSON.parse(readString(inputPtr));
    let callbackResult = null;

    if (input.callbackPath) {
      const kindPtr = writeString(input.callbackKind || "source");
      const dataPtr = writeString(input.callbackPath);
      const contentsPtr = alloc(4);
      const errorPtr = alloc(4);
      setPointer(contentsPtr, 0);
      setPointer(errorPtr, 0);

      callbacks.get(callbackPtr)(context, kindPtr, dataPtr, contentsPtr, errorPtr);

      const contents = getPointer(contentsPtr);
      const error = getPointer(errorPtr);
      callbackResult = contents
        ? { contents: readString(contents) }
        : { error: readString(error) };
    }

    return writeString(JSON.stringify({
      language: input.language,
      callbackResult,
    }));
  },
  Runtime: {
    addFunction(callback, signature) {
      assert.equal(signature, "viiiii");
      callbacks.set(1, callback);
      return 1;
    },
    removeFunction(ptr) {
      assert.equal(ptr, 1);
      callbacks.delete(ptr);
    },
  },
};

const compiler = solar.setupMethods(soljson);
assert.equal(compiler.features.nativeStandardJSON, true);
assert.equal(compiler.features.importCallback, true);

let output = JSON.parse(compiler.compile(JSON.stringify({
  language: "Solidity",
  callbackPath: "B.sol",
})));
assert.deepEqual(output.callbackResult, { error: "File import callback not supported" });
assert.equal(callbacks.size, 0);
assert.equal(resetCount, 1);
assert.ok(freed.length >= 2);

output = JSON.parse(compiler.compile(
  JSON.stringify({ language: "Solidity", callbackPath: "B.sol" }),
  { import(path) { return { contents: `contract B {} // ${path}` }; } },
));
assert.deepEqual(output.callbackResult, { contents: "contract B {} // B.sol" });
assert.equal(resetCount, 2);

output = JSON.parse(compiler.compile(
  JSON.stringify({
    language: "Solidity",
    callbackKind: "smt-query",
    callbackPath: "(check-sat)",
  }),
  { smtSolver(query) { return { contents: `sat ${query}` }; } },
));
assert.deepEqual(output.callbackResult, { contents: "sat (check-sat)" });
assert.equal(resetCount, 3);

assert.throws(
  () => compiler.compile(JSON.stringify({ language: "Solidity" }), () => {}),
  /Invalid callback object specified/,
);
"#
    .replace("__WRAPPER__", &wrapper);

    let output = Command::new("node").arg("-e").arg(script).output().unwrap();
    assert!(
        output.status.success(),
        "node failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
