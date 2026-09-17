//@ codegen-matrix: standard opt
//@ run-call: 0x945bb2ba0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000aa => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000aa
//@ run-call: 0x74016a590000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000aa => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000ffffffffffffffffffffffffffffffffffffffff
//@ run-call: 0x43a4968c0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000aa => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000ffffffffffffffffffffffffffffffffffffffff
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=MIR

// Returning an array parameter re-encodes its elements only when a word wider
// than the element type can have reached them. ABI decoding validates every
// element, so a function that stores nothing wider returns the payload with one
// copy; assembly that stores a full word, here or in a callee, keeps the
// per-element cleanup.
contract AbiReturnArrayElements {
    // MIR-LABEL: fn @clean
    // MIR: mcopy
    // MIR-NOT: and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    // MIR: returndata
    function clean(address[] memory a) external pure returns (address[] memory) {
        return a;
    }

    // MIR-LABEL: fn @dirty
    // MIR: and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    function dirty(address[] memory a) external pure returns (address[] memory) {
        assembly {
            mstore(add(a, 32), not(0))
        }
        return a;
    }

    // MIR-LABEL: fn @viaHelper
    // MIR: and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    function viaHelper(address[] memory a) external pure returns (address[] memory) {
        _scribble(a);
        return a;
    }

    function _scribble(address[] memory a) private pure {
        assembly {
            mstore(add(a, 32), not(0))
        }
    }
}
