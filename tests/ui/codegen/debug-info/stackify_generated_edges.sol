//@ compile-flags: -Onone --evm-version paris --emit=bin,srcmap,srcmap-runtime -Zdump=evm-ir -Zevm-ir-pipeline=none -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "debugify: [^\n]*\n" -> ""
//@ filecheck:

// The jump over constructor helpers and implicit edges between assembler fragments are generated
// control flow. Neither has a source operation, unlike an explicitly located MIR fallthrough.
// CHECK-LABEL: (creation) ===
// CHECK: jump {{bb[0-9]+}} !metadata(compiler_generated)
contract GeneratedEdges {
    uint256 public value;

    constructor(uint256 initial) {
        value = helper(initial);
    }

    function helper(uint256 x) private pure returns (uint256) {
        return x ^ 23;
    }

    function fail(string memory message) external pure {
        revert(message);
    }

    function failAgain(string memory message) external pure {
        revert(message);
    }
}
