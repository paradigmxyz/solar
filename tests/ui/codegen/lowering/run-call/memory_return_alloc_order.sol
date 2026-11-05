//@ codegen-matrix: standard
//@ run-call: structOrder => 64
//@ run-call: staticArrayOrder => 64
//@ run-call: dynamicArrayZeroSlot => 96
//@ run-call: precedingAllocationKept => 5, 7

// A memory struct or static array return variable is allocated at function entry, like solc,
// not at its first use: assembly that reads and bumps the free memory pointer before touching the
// variable gets memory after it. Dynamic arrays point at the zero slot and allocate nothing.
// Reduced from DN404's `_packedLogsMalloc`, which writes below the memory it bumps past.
contract C {
    struct P {
        uint256 a;
        uint256 b;
    }

    struct T {
        uint256 c;
        uint256 d;
    }

    function makeStruct() internal pure returns (P memory p) {
        assembly {
            let q := mload(0x40)
            mstore(0x40, add(q, 0x20))
            mstore(p, q)
        }
    }

    function makeStaticArray() internal pure returns (uint256[2] memory a) {
        assembly {
            let q := mload(0x40)
            mstore(0x40, add(q, 0x20))
            mstore(a, q)
        }
    }

    function makeDynamicArray() internal pure returns (uint256[] memory a, uint256 fmp) {
        assembly {
            fmp := mload(0x40)
            mstore(0x40, add(fmp, 0x20))
        }
    }

    function structOrder() external pure returns (uint256) {
        P memory p = makeStruct();
        uint256 addr;
        assembly {
            addr := p
        }
        return p.a - addr;
    }

    function staticArrayOrder() external pure returns (uint256) {
        uint256[2] memory a = makeStaticArray();
        uint256 addr;
        assembly {
            addr := a
        }
        return a[0] - addr;
    }

    function dynamicArrayZeroSlot() external pure returns (uint256 addr) {
        (uint256[] memory a,) = makeDynamicArray();
        assembly {
            addr := a
        }
    }

    function precedingAllocationKept() external pure returns (uint256, uint256) {
        T memory t;
        t.c = 5;
        t.d = 7;
        P memory p = makeStruct();
        assembly {
            // Writes the two words below the memory `makeStruct` bumped past, which solc
            // places inside `p`, not `t`.
            let q := mload(p)
            mstore(sub(q, 0x40), 0xdead)
            mstore(sub(q, 0x20), 0xbeef)
        }
        return (t.c, t.d);
    }
}
