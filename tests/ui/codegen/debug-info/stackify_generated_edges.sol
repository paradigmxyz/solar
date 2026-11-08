//@ compile-flags: -Onone --evm-version paris --emit=bin,srcmap,srcmap-runtime -Zdump=evm-ir -Zevm-ir-pipeline=none -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "debugify: [^\n]*\n" -> ""
//@ filecheck:

// The jump over constructor helpers is part of entering the constructor, so it maps to the
// constructor's declaration. See `constructor_entry_jump_origin.sol`.
// CHECK-LABEL: (creation) ===
// CHECK: jump {{bb[0-9]+}} !metadata(span={{[0-9]+}}..{{[0-9]+}})
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
