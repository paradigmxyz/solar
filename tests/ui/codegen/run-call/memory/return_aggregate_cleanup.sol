//@ codegen-matrix: standard
//@ run-call: dirtyStruct => (0x00000000000000000000000000000000000000ff, 7)
//@ run-call: dirtyFixedArray => [0x00000000000000000000000000000000000000ff, 0x0000000000000000000000000000000000000001]
//@ run-call: dirtyNested => [(0x00000000000000000000000000000000000000ff, 7)]
//@ run-call: sharedDirtyChildren => [[0x00000000000000000000000000000000000000ff, 0x0000000000000000000000000000000000000001], [0x00000000000000000000000000000000000000ff, 0x0000000000000000000000000000000000000001]]
//@ run-call-fail: badEnum => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

// Returned memory aggregates are cleaned once, by the encoder, as it reads each
// word: dirty high bits that assembly left in an address field disappear, and an
// out-of-range enum still panics.
contract ReturnAggregateCleanup {
    enum Kind {
        A,
        B
    }

    struct Pair {
        address account;
        uint256 value;
    }

    struct Tagged {
        Kind kind;
    }

    function dirtyStruct() external pure returns (Pair memory p) {
        p.value = 7;
        assembly {
            mstore(p, not(0xff00))
        }
        p.account = address(uint160(uint256(uint160(p.account)) & 0xff));
        assembly {
            mstore(p, or(mload(p), shl(200, 1)))
        }
    }

    function dirtyFixedArray() external pure returns (address[2] memory a) {
        assembly {
            mstore(a, or(0xff, shl(200, 1)))
            mstore(add(a, 32), or(1, shl(255, 1)))
        }
    }

    function dirtyNested() external pure returns (Pair[] memory list) {
        list = new Pair[](1);
        list[0].value = 7;
        Pair memory p = list[0];
        assembly {
            mstore(p, or(0xff, shl(160, 0xabc)))
        }
    }

    function sharedDirtyChildren() external pure returns (address[][] memory list) {
        address[] memory child = new address[](2);
        list = new address[][](2);
        list[0] = child;
        list[1] = child;
        assembly {
            mstore(add(child, 32), or(0xff, shl(160, 0xabc)))
            mstore(add(child, 64), or(1, shl(255, 1)))
        }
    }

    function badEnum() external pure returns (Tagged memory t) {
        assembly {
            mstore(t, 5)
        }
    }
}
