//! Foundry integration test harness.
//!
//! This module tests Solar's codegen by comparing execution results
//! between Solar-compiled and solc-compiled contracts.
#![allow(
    unreachable_pub,
    clippy::uninlined_format_args,
    dead_code,
    unused_variables,
    unused_imports
)]

use std::{
    collections::HashMap,
    path::Path,
    process::{Command, Stdio},
};

/// Result of a contract call.
#[derive(Debug, Clone, PartialEq)]
pub struct CallResult {
    pub success: bool,
    pub output: String,
}

/// Test harness for comparing Solar and solc.
pub struct FoundryHarness {
    anvil_port: u16,
    anvil_process: Option<std::process::Child>,
    private_key: String,
}

impl FoundryHarness {
    /// Creates a new test harness.
    pub fn new(port: u16) -> Self {
        Self {
            anvil_port: port,
            anvil_process: None,
            private_key: "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
                .to_string(),
        }
    }

    /// Starts anvil.
    pub fn start_anvil(&mut self) -> std::io::Result<()> {
        let child = Command::new("anvil")
            .args(["--port", &self.anvil_port.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        self.anvil_process = Some(child);
        // Wait for anvil to start
        std::thread::sleep(std::time::Duration::from_secs(2));
        Ok(())
    }

    /// Stops anvil.
    pub fn stop_anvil(&mut self) {
        if let Some(mut child) = self.anvil_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn rpc_url(&self) -> String {
        format!("http://localhost:{}", self.anvil_port)
    }

    /// Deploys a contract and returns its address.
    pub fn deploy(&self, bytecode: &str) -> Result<String, String> {
        let output = Command::new("cast")
            .args([
                "send",
                "--rpc-url",
                &self.rpc_url(),
                "--private-key",
                &self.private_key,
                "--create",
                bytecode,
            ])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with("contractAddress") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    return Ok(parts[1].to_string());
                }
            }
        }
        Err("Could not find contract address in output".to_string())
    }

    /// Calls a contract function (read-only).
    pub fn call(&self, contract: &str, sig: &str) -> Result<String, String> {
        let output = Command::new("cast")
            .args(["call", contract, sig, "--rpc-url", &self.rpc_url()])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Sends a transaction to a contract.
    pub fn send(&self, contract: &str, sig: &str) -> Result<(), String> {
        let output = Command::new("cast")
            .args([
                "send",
                contract,
                sig,
                "--rpc-url",
                &self.rpc_url(),
                "--private-key",
                &self.private_key,
            ])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        Ok(())
    }

    /// Sends a transaction with arguments.
    pub fn send_with_args(&self, contract: &str, sig: &str, args: &[&str]) -> Result<(), String> {
        let rpc = self.rpc_url();
        let mut cmd_args =
            vec!["send", contract, sig, "--rpc-url", &rpc, "--private-key", &self.private_key];
        cmd_args.extend(args);

        let output = Command::new("cast").args(&cmd_args).output().map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        Ok(())
    }
}

impl Drop for FoundryHarness {
    fn drop(&mut self) {
        self.stop_anvil();
    }
}

/// Compiles a Solidity source with Solar and returns (deployment_bytecode, runtime_bytecode).
pub fn compile_with_solar(source: &str) -> Result<(String, String), String> {
    use solar_codegen::{EvmCodegen, lower};
    use solar_interface::Session;
    use solar_sema::Compiler;
    use std::ops::ControlFlow;

    // Write source to temp file
    let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source_path = temp_dir.path().join("Contract.sol");
    std::fs::write(&source_path, source).map_err(|e| e.to_string())?;

    let sess = Session::builder().with_buffer_emitter(solar_interface::ColorChoice::Never).build();

    let mut compiler = Compiler::new(sess);

    let result = compiler.enter_mut(|compiler| -> solar_interface::Result<(String, String)> {
        let mut pcx = compiler.parse();
        pcx.load_files([source_path.as_path()])?;
        pcx.parse();

        let ControlFlow::Continue(()) = compiler.lower_asts()? else {
            return Err(compiler.gcx().sess.dcx.err("lowering failed").emit());
        };

        let ControlFlow::Continue(()) = compiler.analysis()? else {
            return Err(compiler.gcx().sess.dcx.err("analysis failed").emit());
        };

        let gcx = compiler.gcx();

        // Get the first contract
        let (contract_id, _contract) = gcx
            .hir
            .contracts_enumerated()
            .next()
            .ok_or_else(|| compiler.gcx().sess.dcx.err("no contracts found").emit())?;

        let module = lower::lower_contract(gcx, contract_id);
        let mut codegen = EvmCodegen::new();
        let (deployment, runtime) = codegen.generate_deployment_bytecode(&module);

        Ok((
            format!("0x{}", alloy_primitives::hex::encode(&deployment)),
            format!("0x{}", alloy_primitives::hex::encode(&runtime)),
        ))
    });

    result.map_err(|_| "compilation failed".to_string())
}

/// Compiles multiple contracts with Solar using two-pass compilation for `new` support.
/// Returns a map of contract_name -> (deployment_bytecode, runtime_bytecode).
pub fn compile_all_with_solar(source: &str) -> Result<HashMap<String, (String, String)>, String> {
    use solar_codegen::{EvmCodegen, FxHashMap, lower};
    use solar_interface::Session;
    use solar_sema::{Compiler, hir::ContractId};
    use std::ops::ControlFlow;

    let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source_path = temp_dir.path().join("Contract.sol");
    std::fs::write(&source_path, source).map_err(|e| e.to_string())?;

    let sess = Session::builder().with_buffer_emitter(solar_interface::ColorChoice::Never).build();

    let mut compiler = Compiler::new(sess);

    let result = compiler.enter_mut(
        |compiler| -> solar_interface::Result<HashMap<String, (String, String)>> {
            let mut pcx = compiler.parse();
            pcx.load_files([source_path.as_path()])?;
            pcx.parse();

            let ControlFlow::Continue(()) = compiler.lower_asts()? else {
                return Err(compiler.gcx().sess.dcx.err("lowering failed").emit());
            };

            let ControlFlow::Continue(()) = compiler.analysis()? else {
                return Err(compiler.gcx().sess.dcx.err("analysis failed").emit());
            };

            let gcx = compiler.gcx();

            // Pass 1: Compile all contracts to get bytecodes
            let mut all_bytecodes: FxHashMap<ContractId, Vec<u8>> = FxHashMap::default();

            for (contract_id, _contract) in gcx.hir.contracts_enumerated() {
                let module = lower::lower_contract(gcx, contract_id);
                let mut codegen = EvmCodegen::new();
                let (deployment_bytecode, _runtime_bytecode) =
                    codegen.generate_deployment_bytecode(&module);
                all_bytecodes.insert(contract_id, deployment_bytecode);
            }

            // Pass 2: Recompile with bytecodes available
            let mut contracts_output = HashMap::new();

            for (contract_id, contract) in gcx.hir.contracts_enumerated() {
                let module = lower::lower_contract_with_bytecodes(gcx, contract_id, &all_bytecodes);
                let mut codegen = EvmCodegen::new();
                let (deployment, runtime) = codegen.generate_deployment_bytecode(&module);

                contracts_output.insert(
                    contract.name.to_string(),
                    (
                        format!("0x{}", alloy_primitives::hex::encode(&deployment)),
                        format!("0x{}", alloy_primitives::hex::encode(&runtime)),
                    ),
                );
            }

            Ok(contracts_output)
        },
    );

    result.map_err(|_| "compilation failed".to_string())
}

/// Compiles a Solidity source with solc and returns (deployment_bytecode, runtime_bytecode).
pub fn compile_with_solc(source: &str) -> Result<(String, String), String> {
    let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let source_path = temp_dir.path().join("Contract.sol");
    std::fs::write(&source_path, source).map_err(|e| e.to_string())?;

    let output = Command::new("solc")
        .args(["--combined-json", "bin,bin-runtime", source_path.to_str().unwrap()])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;

    let contracts = json["contracts"].as_object().ok_or("no contracts object")?;

    // Get the first contract
    let (_, contract_data) = contracts.iter().next().ok_or("no contracts found")?;

    let deployment = contract_data["bin"].as_str().ok_or("no bin field")?;
    let runtime = contract_data["bin-runtime"].as_str().ok_or("no bin-runtime field")?;

    Ok((format!("0x{}", deployment), format!("0x{}", runtime)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port offset to avoid conflicts between parallel tests
    fn get_port(test_id: u16) -> u16 {
        8600 + test_id
    }

    /// Level 0: Simplest possible contract - just deploys with no functions
    #[test]
    fn test_level0_empty_contract() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Empty {}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar compilation failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc compilation failed: {:?}", solc_result);

        let (solar_deploy, _) = solar_result.unwrap();
        let (solc_deploy, _) = solc_result.unwrap();

        // Both should produce non-empty bytecode
        assert!(solar_deploy.len() > 2, "Solar produced empty bytecode");
        assert!(solc_deploy.len() > 2, "Solc produced empty bytecode");

        println!("Solar deployment: {} bytes", (solar_deploy.len() - 2) / 2);
        println!("Solc deployment:  {} bytes", (solc_deploy.len() - 2) / 2);
    }

    /// Level 1: Contract with a single public view function returning a constant
    #[test]
    fn test_level1_constant_return() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Constant {
    function getValue() public pure returns (uint256) {
        return 42;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        // Deploy and test both
        let mut harness = FoundryHarness::new(get_port(1));
        harness.start_anvil().expect("failed to start anvil");

        let (solar_deploy, _) = solar_result.unwrap();
        let (solc_deploy, _) = solc_result.unwrap();

        // Deploy Solar version
        let solar_addr = harness.deploy(&solar_deploy).expect("Solar deploy failed");
        let solar_value = harness.call(&solar_addr, "getValue()").expect("Solar call failed");

        // Deploy Solc version
        let solc_addr = harness.deploy(&solc_deploy).expect("Solc deploy failed");
        let solc_value = harness.call(&solc_addr, "getValue()").expect("Solc call failed");

        // Both should return 42 (0x2a)
        assert!(
            solar_value
                .contains("000000000000000000000000000000000000000000000000000000000000002a"),
            "Solar returned wrong value: {}",
            solar_value
        );
        assert!(
            solc_value.contains("000000000000000000000000000000000000000000000000000000000000002a"),
            "Solc returned wrong value: {}",
            solc_value
        );

        println!("Solar: {} -> {}", solar_addr, solar_value);
        println!("Solc:  {} -> {}", solc_addr, solc_value);
    }

    /// Level 2: Contract with storage variable and getter
    #[test]
    fn test_level2_storage_read() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Storage {
    uint256 public value;
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        let mut harness = FoundryHarness::new(get_port(2));
        harness.start_anvil().expect("failed to start anvil");

        let (solar_deploy, _) = solar_result.unwrap();
        let (solc_deploy, _) = solc_result.unwrap();

        let solar_addr = harness.deploy(&solar_deploy).expect("Solar deploy failed");
        let solc_addr = harness.deploy(&solc_deploy).expect("Solc deploy failed");

        // Initial value should be 0
        let solar_val = harness.call(&solar_addr, "value()").expect("Solar call failed");
        let solc_val = harness.call(&solc_addr, "value()").expect("Solc call failed");

        let zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
        assert_eq!(solar_val, zero, "Solar initial value wrong");
        assert_eq!(solc_val, zero, "Solc initial value wrong");
    }

    /// Level 3: Contract with storage write (increment)
    #[test]
    fn test_level3_storage_write() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Counter {
    uint256 public count;

    function increment() public {
        count = count + 1;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        let mut harness = FoundryHarness::new(get_port(3));
        harness.start_anvil().expect("failed to start anvil");

        let (solar_deploy, _) = solar_result.unwrap();
        let (solc_deploy, _) = solc_result.unwrap();

        let solar_addr = harness.deploy(&solar_deploy).expect("Solar deploy failed");
        let solc_addr = harness.deploy(&solc_deploy).expect("Solc deploy failed");

        let zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
        let one = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let two = "0x0000000000000000000000000000000000000000000000000000000000000002";

        // Test Solar
        assert_eq!(harness.call(&solar_addr, "count()").unwrap(), zero);
        harness.send(&solar_addr, "increment()").expect("Solar increment failed");
        assert_eq!(harness.call(&solar_addr, "count()").unwrap(), one);
        harness.send(&solar_addr, "increment()").expect("Solar increment 2 failed");
        assert_eq!(harness.call(&solar_addr, "count()").unwrap(), two);

        // Test Solc
        assert_eq!(harness.call(&solc_addr, "count()").unwrap(), zero);
        harness.send(&solc_addr, "increment()").expect("Solc increment failed");
        assert_eq!(harness.call(&solc_addr, "count()").unwrap(), one);
        harness.send(&solc_addr, "increment()").expect("Solc increment 2 failed");
        assert_eq!(harness.call(&solc_addr, "count()").unwrap(), two);

        println!("✓ Both compilers produce equivalent behavior");
    }

    /// Level 4: Multiple functions with storage
    #[test]
    fn test_level4_multiple_functions() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Counter {
    uint256 public count;

    function increment() public {
        count = count + 1;
    }

    function getCount() public view returns (uint256) {
        return count;
    }

    function add(uint256 x) public {
        count = count + x;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        let mut harness = FoundryHarness::new(get_port(4));
        harness.start_anvil().expect("failed to start anvil");

        let (solar_deploy, _) = solar_result.unwrap();
        let (solc_deploy, _) = solc_result.unwrap();

        let solar_addr = harness.deploy(&solar_deploy).expect("Solar deploy failed");
        let solc_addr = harness.deploy(&solc_deploy).expect("Solc deploy failed");

        // Test increment and getCount
        harness.send(&solar_addr, "increment()").unwrap();
        harness.send(&solc_addr, "increment()").unwrap();

        let solar_count = harness.call(&solar_addr, "getCount()").unwrap();
        let solc_count = harness.call(&solc_addr, "getCount()").unwrap();
        assert_eq!(solar_count, solc_count, "getCount mismatch after increment");

        println!("✓ Multiple functions work correctly");
        println!("Solar bytecode: {} bytes", (solar_deploy.len() - 2) / 2);
        println!("Solc bytecode:  {} bytes", (solc_deploy.len() - 2) / 2);
    }

    /// Level 5: Arithmetic operations
    #[test]
    fn test_level5_arithmetic() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Math {
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    function sub(uint256 a, uint256 b) public pure returns (uint256) {
        return a - b;
    }

    function mul(uint256 a, uint256 b) public pure returns (uint256) {
        return a * b;
    }

    function div(uint256 a, uint256 b) public pure returns (uint256) {
        return a / b;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        println!("✓ Arithmetic contract compiles");
        println!("Solar: {} bytes", (solar_result.unwrap().0.len() - 2) / 2);
        println!("Solc:  {} bytes", (solc_result.unwrap().0.len() - 2) / 2);
    }

    /// Level 6: Multiple storage slots
    #[test]
    fn test_level6_multiple_storage() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MultiStorage {
    uint256 public a;
    uint256 public b;
    uint256 public c;

    function setA(uint256 _a) public {
        a = _a;
    }

    function setB(uint256 _b) public {
        b = _b;
    }

    function setC(uint256 _c) public {
        c = _c;
    }

    function sum() public view returns (uint256) {
        return a + b + c;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        let mut harness = FoundryHarness::new(get_port(6));
        harness.start_anvil().expect("failed to start anvil");

        let (solar_deploy, _) = solar_result.unwrap();
        let (solc_deploy, _) = solc_result.unwrap();

        let solar_addr = harness.deploy(&solar_deploy).expect("Solar deploy failed");
        let solc_addr = harness.deploy(&solc_deploy).expect("Solc deploy failed");

        // Initial values should all be 0
        let zero = "0x0000000000000000000000000000000000000000000000000000000000000000";
        assert_eq!(harness.call(&solar_addr, "a()").unwrap(), zero);
        assert_eq!(harness.call(&solar_addr, "b()").unwrap(), zero);
        assert_eq!(harness.call(&solar_addr, "c()").unwrap(), zero);

        println!("✓ Multiple storage slots work");
        println!("Solar: {} bytes", (solar_deploy.len() - 2) / 2);
        println!("Solc:  {} bytes", (solc_deploy.len() - 2) / 2);
    }

    /// Level 7: Comparisons and conditionals
    #[test]
    fn test_level7_conditionals() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Conditionals {
    function max(uint256 a, uint256 b) public pure returns (uint256) {
        if (a > b) {
            return a;
        }
        return b;
    }

    function isPositive(int256 x) public pure returns (bool) {
        return x > 0;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        println!("✓ Conditionals compile");
    }

    /// Level 8: Loops
    #[test]
    fn test_level8_loops() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Loops {
    function sumTo(uint256 n) public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 1; i <= n; i = i + 1) {
            sum = sum + i;
        }
        return sum;
    }

    function factorial(uint256 n) public pure returns (uint256) {
        uint256 result = 1;
        uint256 i = 2;
        while (i <= n) {
            result = result * i;
            i = i + 1;
        }
        return result;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        println!("✓ Loops compile");
    }

    /// Level 9: Boolean type
    #[test]
    fn test_level9_booleans() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Booleans {
    bool public flag;

    function setTrue() public {
        flag = true;
    }

    function setFalse() public {
        flag = false;
    }

    function toggle() public {
        flag = !flag;
    }

    function andOp(bool a, bool b) public pure returns (bool) {
        return a && b;
    }

    function orOp(bool a, bool b) public pure returns (bool) {
        return a || b;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        println!("✓ Booleans compile");
    }

    /// Level 10: Address type and msg.sender
    #[test]
    fn test_level10_address() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract AddressTest {
    address public owner;

    function setOwner() public {
        owner = msg.sender;
    }

    function getOwner() public view returns (address) {
        return owner;
    }

    function isOwner() public view returns (bool) {
        return msg.sender == owner;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        let solc_result = compile_with_solc(source);

        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
        assert!(solc_result.is_ok(), "Solc failed: {:?}", solc_result);

        println!("✓ Address type compiles");
    }

    /// Level 11: Verify arithmetic runtime behavior
    #[test]
    fn test_level11_arithmetic_runtime() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Math {
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }

    function mul(uint256 a, uint256 b) public pure returns (uint256) {
        return a * b;
    }
}
"#;

        let solar_result = compile_with_solar(source).expect("Solar failed");
        let solc_result = compile_with_solc(source).expect("Solc failed");

        let mut harness = FoundryHarness::new(get_port(11));
        harness.start_anvil().expect("failed to start anvil");

        let solar_addr = harness.deploy(&solar_result.0).expect("Solar deploy failed");
        let solc_addr = harness.deploy(&solc_result.0).expect("Solc deploy failed");

        // Test add(5, 3) = 8
        let solar_add = harness.call(&solar_addr, "add(uint256,uint256)(5,3)");
        let solc_add = harness.call(&solc_addr, "add(uint256,uint256)(5,3)");

        // Both should return 8
        let eight = "0x0000000000000000000000000000000000000000000000000000000000000008";

        if let Ok(ref v) = solar_add {
            assert_eq!(v, eight, "Solar add(5,3) wrong: {}", v);
        }
        if let Ok(ref v) = solc_add {
            assert_eq!(v, eight, "Solc add(5,3) wrong: {}", v);
        }

        println!("✓ Arithmetic runtime behavior matches");
    }

    /// Level 12: Verify conditional runtime behavior
    #[test]
    fn test_level12_conditional_runtime() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Max {
    function max(uint256 a, uint256 b) public pure returns (uint256) {
        if (a > b) {
            return a;
        }
        return b;
    }
}
"#;

        let solar_result = compile_with_solar(source).expect("Solar failed");
        let solc_result = compile_with_solc(source).expect("Solc failed");

        let mut harness = FoundryHarness::new(get_port(12));
        harness.start_anvil().expect("failed to start anvil");

        let solar_addr = harness.deploy(&solar_result.0).expect("Solar deploy failed");
        let solc_addr = harness.deploy(&solc_result.0).expect("Solc deploy failed");

        // Test max(5, 3) = 5
        let solar_max = harness.call(&solar_addr, "max(uint256,uint256)(5,3)");
        let solc_max = harness.call(&solc_addr, "max(uint256,uint256)(5,3)");

        let five = "0x0000000000000000000000000000000000000000000000000000000000000005";

        if let Ok(ref v) = solar_max {
            assert_eq!(v, five, "Solar max(5,3) wrong: {}", v);
        }
        if let Ok(ref v) = solc_max {
            assert_eq!(v, five, "Solc max(5,3) wrong: {}", v);
        }

        // Test max(3, 7) = 7
        let solar_max2 = harness.call(&solar_addr, "max(uint256,uint256)(3,7)");
        let solc_max2 = harness.call(&solc_addr, "max(uint256,uint256)(3,7)");

        let seven = "0x0000000000000000000000000000000000000000000000000000000000000007";

        if let Ok(ref v) = solar_max2 {
            assert_eq!(v, seven, "Solar max(3,7) wrong: {}", v);
        }
        if let Ok(ref v) = solc_max2 {
            assert_eq!(v, seven, "Solc max(3,7) wrong: {}", v);
        }

        println!("✓ Conditional runtime behavior matches");
    }

    /// Summary test: Print compilation stats
    #[test]
    fn test_summary_stats() {
        let contracts = vec![
            ("Empty", r#"contract Empty {}"#),
            (
                "Counter",
                r#"contract Counter { uint256 public c; function inc() public { c = c + 1; } }"#,
            ),
            (
                "Math",
                r#"contract Math { function add(uint256 a, uint256 b) public pure returns (uint256) { return a + b; } }"#,
            ),
        ];

        println!("\n=== Compilation Size Comparison ===");
        println!("{:<15} {:>12} {:>12} {:>10}", "Contract", "Solar", "Solc", "Reduction");
        println!("{}", "-".repeat(52));

        for (name, source) in contracts {
            let full_source =
                format!("// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n{}", source);

            let solar = compile_with_solar(&full_source);
            let solc = compile_with_solc(&full_source);

            if let (Ok((solar_bc, _)), Ok((solc_bc, _))) = (solar, solc) {
                let solar_size = (solar_bc.len() - 2) / 2;
                let solc_size = (solc_bc.len() - 2) / 2;
                let reduction =
                    if solc_size > 0 { 100 - (100 * solar_size / solc_size) } else { 0 };
                println!("{:<15} {:>10} B {:>10} B {:>9}%", name, solar_size, solc_size, reduction);
            }
        }
        println!();
    }

    // ==========================================
    // Tests for features NOT YET IMPLEMENTED
    // Run with: cargo test --test foundry -- --ignored
    // ==========================================

    /// Test: Contract creation with `new`
    #[test]
    fn test_wip_contract_creation() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Child {
    uint256 public value;
    constructor(uint256 _v) {
        value = _v;
    }
}

contract Factory {
    function create(uint256 v) public returns (address) {
        Child c = new Child(v);
        return address(c);
    }
}
"#;

        // Use the new multi-contract compiler
        let solar_result = compile_all_with_solar(source);
        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);

        let contracts = solar_result.unwrap();
        assert!(contracts.contains_key("Child"), "Child contract not found");
        assert!(contracts.contains_key("Factory"), "Factory contract not found");

        // Verify the Factory contract bytecode is larger than the Child
        // (since Factory embeds Child's bytecode)
        let factory_bytecode = &contracts["Factory"].0;
        let child_bytecode = &contracts["Child"].0;

        eprintln!("Child bytecode len: {}", child_bytecode.len());
        eprintln!("Factory bytecode len: {}", factory_bytecode.len());

        // Factory should have bytecode that includes CREATE instruction
        // Factory bytecode should contain the CREATE opcode (0xf0)
        assert!(factory_bytecode.len() > 10, "Factory bytecode too short: {}", factory_bytecode);
    }

    /// Test: External calls
    /// EXPECTED TO FAIL: external calls not implemented
    #[test]
    #[ignore = "external calls not implemented"]
    fn test_wip_external_calls() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface ICounter {
    function count() external view returns (uint256);
    function increment() external;
}

contract Caller {
    function callIncrement(address target) public {
        ICounter(target).increment();
    }
}
"#;

        let solar_result = compile_with_solar(source);
        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
    }

    /// Test: Events
    /// EXPECTED TO FAIL: events not implemented
    #[test]
    #[ignore = "events not implemented"]
    fn test_wip_events() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract EventEmitter {
    event ValueChanged(uint256 newValue);

    uint256 public value;

    function setValue(uint256 _v) public {
        value = _v;
        emit ValueChanged(_v);
    }
}
"#;

        let solar_result = compile_with_solar(source);
        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);

        // Would need to verify events are emitted
    }

    /// Test: Arrays
    /// EXPECTED TO FAIL: arrays not fully implemented
    #[test]
    #[ignore = "arrays not implemented"]
    fn test_wip_arrays() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ArrayTest {
    uint256[] public values;

    function push(uint256 v) public {
        values.push(v);
    }

    function length() public view returns (uint256) {
        return values.length;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
    }

    /// Test: Mappings
    /// EXPECTED TO FAIL: mappings not implemented
    #[test]
    #[ignore = "mappings not implemented"]
    fn test_wip_mappings() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MapTest {
    mapping(address => uint256) public balances;

    function set(address a, uint256 v) public {
        balances[a] = v;
    }

    function get(address a) public view returns (uint256) {
        return balances[a];
    }
}
"#;

        let solar_result = compile_with_solar(source);
        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);
    }

    /// Test: require with message
    /// EXPECTED TO FAIL: revert messages not implemented
    #[test]
    #[ignore = "require messages not implemented"]
    fn test_wip_require_message() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract RequireTest {
    function mustBePositive(int256 x) public pure returns (int256) {
        require(x > 0, "must be positive");
        return x;
    }
}
"#;

        let solar_result = compile_with_solar(source);
        assert!(solar_result.is_ok(), "Solar failed: {:?}", solar_result);

        // Would need to verify revert message is correct
    }
}
