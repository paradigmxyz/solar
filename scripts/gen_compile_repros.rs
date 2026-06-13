#!/usr/bin/env -S cargo +nightly -Zscript
//! Generates minimal reproducible Solidity files for compile-time profiling.
//!
//! Usage: ./scripts/gen_compile_repros.rs [--sizes small|medium|large|all]
//!
//! This generates synthetic Solidity files that stress different parts of the compiler:
//! - many_symbols: Many unique identifiers (symbol table stress)
//! - many_functions: Many function declarations (HIR lowering stress)
//! - deep_nesting: Deeply nested control flow (CFG/MIR stress)
//! - many_types: Many type declarations (type resolution stress)
//! - large_literals: Many large constant expressions (constant folding stress)
//! - many_storage: Many storage variables (storage layout stress)
//! - many_events: Many event declarations (ABI generation stress)
//! - complex_inheritance: Deep inheritance hierarchies (linearization stress)

use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sizes = if args.iter().any(|a| a == "--sizes") {
        args.iter()
            .skip_while(|a| *a != "--sizes")
            .nth(1)
            .map(|s| s.as_str())
            .unwrap_or("all")
    } else {
        "all"
    };

    // CARGO_MANIFEST_DIR points to the scripts directory when run as a script,
    // so we go up one level to the repo root
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(Path::new("."))
        .join("testdata/repros");
    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    let size_configs: Vec<(&str, usize, usize, usize)> = match sizes {
        "small" => vec![("small", 100, 10, 5)],
        "medium" => vec![("medium", 1000, 50, 10)],
        "large" => vec![("large", 10000, 200, 20)],
        _ => vec![
            ("small", 100, 10, 5),
            ("medium", 1000, 50, 10),
            ("large", 10000, 60, 20), // depth limited to 60 to avoid parser recursion limit
        ],
    };

    for (size_name, n_symbols, n_depth, n_types) in size_configs {
        println!("Generating {size_name} repros (n={n_symbols}, depth={n_depth}, types={n_types})...");

        generate_many_symbols(&output_dir, size_name, n_symbols);
        generate_many_functions(&output_dir, size_name, n_symbols);
        generate_deep_nesting(&output_dir, size_name, n_depth);
        generate_many_types(&output_dir, size_name, n_types * 10);
        generate_large_literals(&output_dir, size_name, n_symbols);
        generate_many_storage(&output_dir, size_name, n_symbols);
        generate_many_events(&output_dir, size_name, n_symbols / 10);
        generate_complex_inheritance(&output_dir, size_name, n_depth.min(50));
        generate_many_mappings(&output_dir, size_name, n_symbols / 10);
        generate_many_modifiers(&output_dir, size_name, n_symbols / 10);
    }

    println!("\nGenerated repros in: {}", output_dir.display());
    println!("\nRun benchmarks with:");
    println!("  cargo bench -p solar-bench --bench compile_time");
}

fn generate_many_symbols(dir: &Path, size: &str, n: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");
    content.push_str("contract ManySymbols {\n");

    // Generate many unique local variables in a function
    content.push_str("    function compute() public pure returns (uint256) {\n");
    for i in 0..n {
        content.push_str(&format!("        uint256 var_{i} = {i};\n"));
    }
    content.push_str("        return ");
    for i in 0..n.min(100) {
        if i > 0 {
            content.push_str(" + ");
        }
        content.push_str(&format!("var_{i}"));
    }
    content.push_str(";\n    }\n}\n");

    write_file(dir, &format!("many_symbols_{size}.sol"), &content);
}

fn generate_many_functions(dir: &Path, size: &str, n: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");
    content.push_str("contract ManyFunctions {\n");

    for i in 0..n {
        content.push_str(&format!(
            "    function func_{i}(uint256 x) public pure returns (uint256) {{\n        return x + {i};\n    }}\n\n"
        ));
    }

    content.push_str("}\n");
    write_file(dir, &format!("many_functions_{size}.sol"), &content);
}

fn generate_deep_nesting(dir: &Path, size: &str, depth: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");
    content.push_str("contract DeepNesting {\n");
    content.push_str("    function nested(uint256 x) public pure returns (uint256) {\n");
    content.push_str("        uint256 result = x;\n");

    // Generate deeply nested if-else chains
    for i in 0..depth {
        content.push_str(&"    ".repeat(i + 2));
        content.push_str(&format!("if (result > {i}) {{\n"));
        content.push_str(&"    ".repeat(i + 3));
        content.push_str("result = result + 1;\n");
    }

    // Close all the if blocks
    for i in (0..depth).rev() {
        content.push_str(&"    ".repeat(i + 2));
        content.push_str("}\n");
    }

    content.push_str("        return result;\n    }\n");

    // Generate deeply nested loops
    content.push_str("    function nestedLoops(uint256 n) public pure returns (uint256) {\n");
    content.push_str("        uint256 sum = 0;\n");

    let loop_depth = depth.min(10); // Limit loop depth to avoid gas issues
    for i in 0..loop_depth {
        content.push_str(&"    ".repeat(i + 2));
        content.push_str(&format!("for (uint256 i{i} = 0; i{i} < n; i{i}++) {{\n"));
    }

    content.push_str(&"    ".repeat(loop_depth + 2));
    content.push_str("sum += 1;\n");

    for i in (0..loop_depth).rev() {
        content.push_str(&"    ".repeat(i + 2));
        content.push_str("}\n");
    }

    content.push_str("        return sum;\n    }\n}\n");
    write_file(dir, &format!("deep_nesting_{size}.sol"), &content);
}

fn generate_many_types(dir: &Path, size: &str, n: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");

    // Generate many structs
    for i in 0..n {
        content.push_str(&format!(
            "struct Struct{i} {{\n    uint256 field1;\n    address field2;\n    bytes32 field3;\n}}\n\n"
        ));
    }

    // Generate many enums
    for i in 0..n {
        content.push_str(&format!(
            "enum Enum{i} {{ Value0, Value1, Value2, Value3 }}\n\n"
        ));
    }

    // Generate many user-defined value types
    for i in 0..n {
        content.push_str(&format!("type CustomUint{i} is uint256;\n"));
    }

    // Generate many errors
    content.push('\n');
    for i in 0..n {
        content.push_str(&format!("error Error{i}(uint256 code);\n"));
    }

    // Generate a contract that uses all the types
    content.push_str("\ncontract ManyTypes {\n");
    for i in 0..n.min(100) {
        content.push_str(&format!("    Struct{i} public struct{i};\n"));
        content.push_str(&format!("    Enum{i} public enum{i};\n"));
    }
    content.push_str("}\n");

    write_file(dir, &format!("many_types_{size}.sol"), &content);
}

fn generate_large_literals(dir: &Path, size: &str, n: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");
    content.push_str("contract LargeLiterals {\n");

    // Generate many large constant expressions
    for i in 0..n {
        let large_num = format!("0x{:064x}", (i as u128) * 12345678901234567890_u128 % (u128::MAX / 2));
        content.push_str(&format!("    uint256 public constant CONST_{i} = {large_num};\n"));
    }

    // Generate a function with many literal computations
    content.push_str("\n    function compute() public pure returns (uint256) {\n");
    content.push_str("        uint256 result = 0;\n");
    for i in 0..n.min(500) {
        content.push_str(&format!("        result += CONST_{i};\n"));
    }
    content.push_str("        return result;\n    }\n");

    // Generate string literals
    content.push_str("\n    function getStrings() public pure returns (string memory) {\n");
    content.push_str("        return string(abi.encodePacked(\n");
    for i in 0..n.min(50) {
        if i > 0 {
            content.push_str(",\n");
        }
        content.push_str(&format!(
            "            \"String literal number {i} with some content to make it longer\""
        ));
    }
    content.push_str("\n        ));\n    }\n}\n");

    write_file(dir, &format!("large_literals_{size}.sol"), &content);
}

fn generate_many_storage(dir: &Path, size: &str, n: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");
    content.push_str("contract ManyStorage {\n");

    // Generate many storage variables of different types
    for i in 0..n {
        match i % 5 {
            0 => content.push_str(&format!("    uint256 public var_{i};\n")),
            1 => content.push_str(&format!("    address public addr_{i};\n")),
            2 => content.push_str(&format!("    bytes32 public hash_{i};\n")),
            3 => content.push_str(&format!("    bool public flag_{i};\n")),
            _ => content.push_str(&format!("    uint128 public small_{i};\n")),
        }
    }

    // Generate a function that reads all storage
    content.push_str("\n    function sumAll() public view returns (uint256) {\n");
    content.push_str("        uint256 sum = 0;\n");
    for i in 0..n.min(200) {
        match i % 5 {
            0 => content.push_str(&format!("        sum += var_{i};\n")),
            1 => content.push_str(&format!("        sum += uint256(uint160(addr_{i}));\n")),
            2 => content.push_str(&format!("        sum += uint256(hash_{i});\n")),
            3 => content.push_str(&format!("        sum += flag_{i} ? 1 : 0;\n")),
            _ => content.push_str(&format!("        sum += small_{i};\n")),
        }
    }
    content.push_str("        return sum;\n    }\n}\n");

    write_file(dir, &format!("many_storage_{size}.sol"), &content);
}

fn generate_many_events(dir: &Path, size: &str, n: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");
    content.push_str("contract ManyEvents {\n");

    // Generate many event declarations
    for i in 0..n {
        content.push_str(&format!(
            "    event Event{i}(address indexed sender, uint256 indexed id, bytes32 data);\n"
        ));
    }

    // Generate a function that emits all events
    content.push_str("\n    function emitAll() public {\n");
    for i in 0..n.min(100) {
        content.push_str(&format!(
            "        emit Event{i}(msg.sender, {i}, bytes32(uint256({i})));\n"
        ));
    }
    content.push_str("    }\n}\n");

    write_file(dir, &format!("many_events_{size}.sol"), &content);
}

fn generate_complex_inheritance(dir: &Path, size: &str, depth: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");

    // Generate a diamond inheritance pattern
    content.push_str("contract Base {\n");
    content.push_str("    uint256 public baseValue;\n");
    content.push_str("    function baseFunc() public virtual returns (uint256) { return 0; }\n");
    content.push_str("}\n\n");

    // Generate left branch
    for i in 0..depth {
        if i == 0 {
            content.push_str("contract Left0 is Base {\n");
        } else {
            content.push_str(&format!("contract Left{i} is Left{} {{\n", i - 1));
        }
        content.push_str(&format!("    uint256 public leftValue{i};\n"));
        content.push_str(&format!(
            "    function leftFunc{i}() public pure returns (uint256) {{ return {i}; }}\n"
        ));
        content.push_str("}\n\n");
    }

    // Generate right branch
    for i in 0..depth {
        if i == 0 {
            content.push_str("contract Right0 is Base {\n");
        } else {
            content.push_str(&format!("contract Right{i} is Right{} {{\n", i - 1));
        }
        content.push_str(&format!("    uint256 public rightValue{i};\n"));
        content.push_str(&format!(
            "    function rightFunc{i}() public pure returns (uint256) {{ return {i}; }}\n"
        ));
        content.push_str("}\n\n");
    }

    // Diamond tip
    content.push_str(&format!(
        "contract Diamond is Left{}, Right{} {{\n",
        depth - 1,
        depth - 1
    ));
    content.push_str("    function baseFunc() public pure override returns (uint256) { return 42; }\n");
    content.push_str("    function diamondFunc() public pure returns (uint256) {\n");
    content.push_str("        uint256 sum = 0;\n");
    for i in 0..depth.min(20) {
        content.push_str(&format!("        sum += leftFunc{i}() + rightFunc{i}();\n"));
    }
    content.push_str("        return sum;\n    }\n}\n");

    write_file(dir, &format!("complex_inheritance_{size}.sol"), &content);
}

fn generate_many_mappings(dir: &Path, size: &str, n: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");
    content.push_str("contract ManyMappings {\n");

    // Generate many mapping declarations with various key/value types
    for i in 0..n {
        match i % 4 {
            0 => content.push_str(&format!(
                "    mapping(address => uint256) public map{i};\n"
            )),
            1 => content.push_str(&format!(
                "    mapping(uint256 => mapping(address => uint256)) public nestedMap{i};\n"
            )),
            2 => content.push_str(&format!(
                "    mapping(bytes32 => address) public hashMap{i};\n"
            )),
            _ => content.push_str(&format!(
                "    mapping(address => mapping(uint256 => bool)) public doubleMap{i};\n"
            )),
        }
    }

    // Generate functions that use the mappings
    content.push_str("\n    function setAll(address addr, uint256 val) public {\n");
    for i in 0..n.min(50) {
        match i % 4 {
            0 => content.push_str(&format!("        map{i}[addr] = val;\n")),
            1 => content.push_str(&format!("        nestedMap{i}[val][addr] = val;\n")),
            2 => content.push_str(&format!(
                "        hashMap{i}[keccak256(abi.encode(val))] = addr;\n"
            )),
            _ => content.push_str(&format!("        doubleMap{i}[addr][val] = true;\n")),
        }
    }
    content.push_str("    }\n}\n");

    write_file(dir, &format!("many_mappings_{size}.sol"), &content);
}

fn generate_many_modifiers(dir: &Path, size: &str, n: usize) {
    let mut content = String::from("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n\n");
    content.push_str("contract ManyModifiers {\n");
    content.push_str("    address public owner;\n");
    content.push_str("    uint256 public nonce;\n\n");

    // Generate many modifier declarations
    for i in 0..n {
        content.push_str(&format!(
            "    modifier mod{i}(uint256 val) {{\n        require(val > {i}, \"mod{i}\");\n        _;\n    }}\n\n"
        ));
    }

    // Generate a function with many modifiers applied
    content.push_str("    function multiModified(uint256 x) public\n");
    for i in 0..n.min(20) {
        content.push_str(&format!("        mod{i}(x)\n"));
    }
    content.push_str("        returns (uint256)\n    {\n        return x;\n    }\n}\n");

    write_file(dir, &format!("many_modifiers_{size}.sol"), &content);
}

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    let mut file = fs::File::create(&path).expect(&format!("Failed to create {}", path.display()));
    file.write_all(content.as_bytes())
        .expect(&format!("Failed to write {}", path.display()));
    println!("  Generated: {name} ({} bytes, {} lines)", content.len(), content.lines().count());
}
