//@ codegen-matrix: standard opt
//@ run-call: aliasedStruct [0] => 1
//@ run-call: aliasedBytes [7] => 7
//@ run-call: unalignedArray [7, 9] => 9
//@ run-call: unalignedHelper [7, 9] => 9
//@ run-call: 0x945bb2ba0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000aa => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000aa
//@ run-call: 0x74016a590000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000aa => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000ffffffffffffffffffffffffffffffffffffffff
//@ run-call: 0x43a4968c0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000aa => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000ffffffffffffffffffffffffffffffffffffffff
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck: --check-prefix=MIR

// Returning an array parameter re-encodes its elements only when a word wider
// than the element type can have reached them. ABI decoding validates every
// element, so a function that stores nothing wider returns the payload in
// place; assembly that stores a full word, here or in a callee, keeps the
// per-element cleanup.
contract AbiReturnArrayElements {
    struct Word {
        uint256 value;
    }

    // MIR-LABEL: fn @clean
    // MIR-NOT: cleanup_return
    // MIR: [[HEAD:v[0-9]+]] = sub {{v[0-9]+}}, 32
    // MIR-NEXT: mstore [[HEAD]], 32
    // MIR-NOT: and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff{{$}}
    // MIR: returndata [[HEAD]],
    function clean(address[] memory a) external pure returns (address[] memory) {
        return a;
    }

    // MIR-LABEL: fn @dirty
    // MIR: icall @cleanup_return
    function dirty(address[] memory a) external pure returns (address[] memory) {
        assembly {
            mstore(add(a, 32), not(0))
        }
        return a;
    }

    // MIR-LABEL: fn @viaHelper
    // MIR: icall @cleanup_return
    function viaHelper(address[] memory a) external pure returns (address[] memory) {
        _scribble(a);
        return a;
    }

    function _scribble(address[] memory a) private pure {
        assembly {
            mstore(add(a, 32), not(0))
        }
    }
    function aliasedStruct(uint8[] memory a) external pure returns (uint256) {
        Word memory word;
        assembly { word := add(a, 32) }
        word.value = 257;
        return a[0];
    }

    function aliasedBytes(uint8[] memory a) external pure returns (uint256) {
        bytes memory data;
        assembly { data := a }
        data[0] = 0x01;
        return a[0];
    }

    function unalignedArray(uint8[] memory a) external pure returns (uint256) {
        uint8[1] memory other;
        assembly { other := add(a, 33) }
        other[0] = 1;
        return a[1];
    }

    function unalignedHelper(uint8[] memory a) external pure returns (uint256) {
        uint8[1] memory other;
        assembly { other := add(a, 33) }
        _writeElement(other);
        return a[1];
    }

    function _writeElement(uint8[1] memory a) private pure {
        a[0] = 1;
    }
}
