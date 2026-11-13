//@ codegen-matrix: standard
//@ compile-flags: --no-warnings
//@ run-call: f() => 0x80

// The heap-floor pass reads `mstore(0x40, mload(0x40))` as a pointer read, because the loaded word
// becomes the pointer again, so it clamps the first store at the store and the data read sees the
// heap floor. solc returns 3 here, as it never raises the slot (CODEGEN-030, CODEGEN-010).
contract C {
    function f() external pure returns (uint256) {
        assembly {
            mstore(0x40, 3)
            mstore(0x40, mload(0x40))
            return(0x40, 0x20)
        }
    }
}
