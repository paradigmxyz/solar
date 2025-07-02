#![allow(unused_crate_dependencies)]

use solar_interface::{
    diagnostics::EmittedDiagnostics,
    source_map::{FileName, SourceFile, SourceFileHashAlgorithm},
    Result, Session,
};
use std::sync::Arc;
use thread_local::ThreadLocal;

#[test]
fn test_pretty_print() -> Result<(), EmittedDiagnostics> {
    // Create a new session with a test emitter
    let dcx = solar_interface::diagnostics::DiagCtxt::with_test_emitter(None);
    let sess = Session::empty(dcx);
    let dcx = &sess.dcx;

    sess.enter(|| {
        let src = r#"
contract Test {
    uint public value;

    constructor(uint _value) {
        value = _value;
    }

    function getValue() public view returns (uint) {
        return value;
    }

    function setValue(uint _value) public {
        value = _value;
    }
}
"#;

        // Create the source file
        let file = match SourceFile::new(
            FileName::custom("test.sol"),
            src.to_string(),
            SourceFileHashAlgorithm::default(),
        ) {
            Ok(file) => Arc::new(file),
            Err(e) => {
                let _ = dcx.err(format!("{e:?}")).emit();
                return Err(dcx.emitted_diagnostics().unwrap());
            }
        };

        // Parse and lower to HIR
        let mut pcx = solar_sema::ParsingContext::new(&sess);
        pcx.add_file(file);
        let hir_arena = ThreadLocal::new();
        let gcx = pcx.parse_and_lower(&hir_arena).map_err(|e| {
            let _ = dcx.err(format!("{e:?}")).emit();
            dcx.emitted_diagnostics().unwrap()
        })?;
        let gcx = match gcx {
            Some(gcx) => gcx,
            None => return Err(dcx.emitted_diagnostics().unwrap()),
        };
        let hir = &gcx.get().hir;

        // Pretty-print the HIR
        let output = hir.pretty_print();

        // Print the output for inspection
        println!("=== HIR Pretty Printer Output ===");
        println!("{output}");
        println!("=== End Output ===");

        // Assert that the output contains expected elements
        assert!(output.contains("contract Test"), "Pretty printer output: {output}");
        assert!(output.contains("getValue()"), "Pretty printer output: {output}");
        assert!(output.contains("setValue("), "Pretty printer output: {output}");

        Ok(())
    })
}

#[test]
fn test_pretty_print_complex() -> Result<(), EmittedDiagnostics> {
    // Create a new session with a test emitter
    let dcx = solar_interface::diagnostics::DiagCtxt::with_test_emitter(None);
    let sess = Session::empty(dcx);
    let dcx = &sess.dcx;

    sess.enter(|| {
        let src = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ComplexTest {
    struct User {
        string name;
        uint256 balance;
        bool active;
    }
    
    enum Status { Pending, Active, Inactive }
    
    event UserCreated(address indexed user, string name);
    error InsufficientBalance(uint256 required, uint256 available);
    
    mapping(address => User) public users;
    Status public status;
    
    modifier onlyActive() {
        require(users[msg.sender].active, "User not active");
        _;
    }
    
    function createUser(string memory name) public {
        users[msg.sender] = User(name, 0, true);
        emit UserCreated(msg.sender, name);
    }
    
    function getUser(address user) public view returns (User memory) {
        return users[user];
    }
    
    function updateStatus(Status newStatus) public onlyActive {
        status = newStatus;
    }
}
"#;

        // Create the source file
        let file = match SourceFile::new(
            FileName::custom("test.sol"),
            src.to_string(),
            SourceFileHashAlgorithm::default(),
        ) {
            Ok(file) => Arc::new(file),
            Err(e) => {
                let _ = dcx.err(format!("{e:?}")).emit();
                return Err(dcx.emitted_diagnostics().unwrap());
            }
        };

        // Parse and lower to HIR
        let mut pcx = solar_sema::ParsingContext::new(&sess);
        pcx.add_file(file);
        let hir_arena = ThreadLocal::new();
        let gcx = pcx.parse_and_lower(&hir_arena).map_err(|e| {
            let _ = dcx.err(format!("{e:?}")).emit();
            dcx.emitted_diagnostics().unwrap()
        })?;
        let gcx = match gcx {
            Some(gcx) => gcx,
            None => return Err(dcx.emitted_diagnostics().unwrap()),
        };
        let hir = &gcx.get().hir;

        // Pretty-print the HIR
        let output = hir.pretty_print();

        // Print the output for inspection
        println!("=== Complex HIR Pretty Printer Output ===");
        println!("{output}");
        println!("=== End Complex Output ===");

        // Assert that the output contains expected elements
        assert!(output.contains("contract ComplexTest"), "Pretty printer output: {output}");
        assert!(output.contains("struct User"), "Pretty printer output: {output}");
        assert!(output.contains("enum Status"), "Pretty printer output: {output}");
        assert!(output.contains("event UserCreated"), "Pretty printer output: {output}");
        assert!(output.contains("error InsufficientBalance"), "Pretty printer output: {output}");
        assert!(output.contains("mapping(address => User)"), "Pretty printer output: {output}");
        assert!(output.contains("Status status"), "Pretty printer output: {output}");
        assert!(output.contains("onlyActive()"), "Pretty printer output: {output}");
        assert!(output.contains("createUser("), "Pretty printer output: {output}");
        assert!(output.contains("getUser("), "Pretty printer output: {output}");
        assert!(output.contains("updateStatus("), "Pretty printer output: {output}");
        assert!(output.contains("indexed address user"), "Pretty printer output: {output}");
        assert!(output.contains("returns (User)"), "Pretty printer output: {output}");
        assert!(output.contains("returns (Status)"), "Pretty printer output: {output}");

        Ok(())
    })
}
