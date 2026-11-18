//@ codegen-matrix: standard
//@ run-call-fail: f 0x0102 => 0x

// A slice that assembly pointed at an address with the top bit set is out of range, so the copy
// to memory reverts instead of reading zeros past the end of calldata.
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
