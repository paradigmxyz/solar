#!/usr/bin/env node
"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const distDir = path.resolve(process.argv[2] || "target/dist");
const packedPath = path.join(distDir, "soljson.js");
const wrapperPath = path.join(distDir, "soljson-wrapper.js");
const wasmPath = path.join(distDir, "solar.wasm");

function standardJsonInput(imports) {
  return {
    language: "Solidity",
    sources: {
      "A.sol": {
        content: imports
          ? 'import "B.sol"; contract A is B { function answer() public pure returns (uint256) { return 42; } }'
          : "contract A { function answer() public pure returns (uint256) { return 42; } }",
      },
    },
    settings: {
      outputSelection: {
        "*": {
          "*": ["abi", "evm.methodIdentifiers"],
        },
      },
    },
  };
}

function callbacks() {
  return {
    import(importPath) {
      if (importPath === "B.sol") {
        return { contents: "contract B {}" };
      }
      return { error: `source not found: ${importPath}` };
    },
  };
}

function assertCompiler(compiler, label, imports) {
  assert.equal(typeof compiler.compile, "function", `${label}: compile`);
  assert.equal(typeof compiler.version(), "string", `${label}: version`);
  assert.equal(compiler.features.nativeStandardJSON, true, `${label}: standard-json`);

  const output = JSON.parse(compiler.compile(JSON.stringify(standardJsonInput(imports)), callbacks()));
  const errors = (output.errors || []).filter((error) => error.severity === "error");
  assert.deepEqual(errors, [], `${label}: no compile errors`);
  assert.equal(
    output.contracts?.["A.sol"]?.A?.evm?.methodIdentifiers?.["answer()"],
    "85bb7d69",
    `${label}: method selector`,
  );
}

async function main() {
  const packed = require(packedPath);
  assert.equal(packed.features.importCallback, true, "packed soljson: import callback");
  assertCompiler(packed, "packed soljson", true);

  const wrapper = require(wrapperPath);
  const wasm = fs.readFileSync(wasmPath);
  const { instance } = await WebAssembly.instantiate(wasm, {});
  assertCompiler(
    wrapper.setupMethods({
      instance,
      exports: instance.exports,
      memory: instance.exports.memory,
    }),
    "raw wasm",
    false,
  );
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
