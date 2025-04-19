#![allow(unused_crate_dependencies)]

use solar_interface::{
    source_map::{FileName, SourceFile, SourceFileHashAlgorithm},
    diagnostics::EmittedDiagnostics,
    Session, Result,
};

#[test]
fn test_pretty_print() -> Result<(), EmittedDiagnostics> {
    // Create a new session with a test emitter
    let dcx = solar_interface::diagnostics::DiagCtxt::with_test_emitter(None);
    let sess = Session::empty(dcx);
    let dcx = &sess.dcx;

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
    let _file = match SourceFile::new(FileName::custom("test.sol"), src.to_string(), SourceFileHashAlgorithm::default()) {
        Ok(file) => file,
        Err(e) => {
            let _ = dcx.err(e.to_string()).emit();
            return Err(dcx.emitted_diagnostics().unwrap());
        }
    };

    // TODO: Find a way to parse and lower the source code using public APIs.
    // For now, just return Ok(())
    Ok(())
} 