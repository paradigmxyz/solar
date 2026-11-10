//@ codegen-matrix: standard
//@ run-call: f 0x0102 => 0x0000000000000000000000000000000000000000000000000000000000000000

// solc reverts with empty data; see CODEGEN-017 in docs/SOLC_DIVERGENCE.md.
contract C {
    function take(bytes memory b) internal pure returns (bytes32 r) {
        assembly {
            r := mload(add(b, 32))
        }
    }

    function f(bytes calldata b) external pure returns (bytes32) {
        assembly {
            b.offset := shl(255, 1)
        }
        return take(b);
    }
}
