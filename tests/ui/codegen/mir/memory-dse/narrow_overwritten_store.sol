//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: hashU8 1, 0x0000000000000000000000000000000000000000000000000000000000000002 => 0x8a2fdc76316025471d7987c83d06026739af92a97cdab6903f02a3bc8b12b2a3
//@ run-call: hashU8 255, 0x0000000000000000000000000000000000000000000000000000000000000007 => 0x67fd2bc151d01a3ba50b41f3fc992e32534677d9448b7e3963ee256c917d918a
//@ run-call: hashConst 0x0000000000000000000000000000000000000000000000000000000000000002 => 0x98b054a7adc6046f16a630da24e8c85772f122f8a92220f89af6efbfacb3b76d
//@ run-call: hashWide 0x1ff, 0x0000000000000000000000000000000000000000000000000000000000000002 => 0x422ab2f2e3bdab887ab6b4f1ae217027537b2abe53886b4d258ec8de35cf2aa9
//@ run-call: crossBlock 0x1ff, 0x0000000000000000000000000000000000000000000000000000000000000002, true => 0x422ab2f2e3bdab887ab6b4f1ae217027537b2abe53886b4d258ec8de35cf2aa9
//@ run-call: crossBlock 0x1ff, 0x0000000000000000000000000000000000000000000000000000000000000002, false => 0x422ab2f2e3bdab887ab6b4f1ae217027537b2abe53886b4d258ec8de35cf2aa9
//@ run-call: tailRead 0x1ff, 3 => 0xff
//@ run-call: literal 5 => 0x6100000000000000000000000000000000000000000000000000000000000000, 5

contract NarrowOverwrittenStore {
    // The shifted byte is the top byte of the first store, and the second store overwrites the rest.
    // CHECK-LABEL: fn @hashU8
    // CHECK-NOT: shl
    // CHECK: mstore8 0, arg0
    function hashU8(uint8 a, bytes32 x) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, x));
    }

    // The shifted operand needs no range fact: `mstore8` keeps its low byte, as the shift does.
    // CHECK-LABEL: fn @hashWide
    // CHECK-NOT: shl
    // CHECK: mstore8 128, arg0
    function hashWide(uint256 a, bytes32 x) external pure returns (bytes32 h) {
        assembly {
            mstore(0x80, shl(248, a))
            mstore(0x81, x)
            h := keccak256(0x80, 33)
        }
    }

    // The store before the branch is dead once the word at 0x80 is overwritten after it, so
    // narrowing waits until that cross-block removal has run.
    // CHECK-LABEL: fn @crossBlock
    // CHECK-NOT: mstore 128, 7
    function crossBlock(uint256 a, bytes32 x, bool b) external returns (bytes32 h) {
        assembly {
            mstore(0x80, 7)
            if b { sstore(0, 1) }
            mstore(0x80, shl(248, a))
            mstore(0x81, x)
            h := keccak256(0x80, 33)
        }
    }

    // A constant with only its top byte set narrows to that byte.
    // CHECK-LABEL: fn @hashConst
    // CHECK: mstore8 128, 97
    function hashConst(bytes32 x) external pure returns (bytes32 h) {
        assembly {
            mstore(0x80, shl(248, 0x61))
            mstore(0x81, x)
            h := keccak256(0x80, 33)
        }
    }

    // `mload(0x81)` reads the tail of the first store before the second overwrites it.
    // CHECK-LABEL: fn @tailRead
    // CHECK-NOT: mstore8
    function tailRead(uint256 a, uint256 x) external pure returns (uint256) {
        assembly {
            mstore(0x80, not(0))
            mstore(0xa0, not(0))
            mstore(0x80, shl(248, a))
            mstore(0xc0, mload(0x81))
            mstore(0x81, x)
            return(0xc0, 32)
        }
    }

    // The zero bytes after the literal are returned, so the store stays whole.
    // CHECK-LABEL: fn @literal
    // CHECK-NOT: mstore8
    function literal(uint256 x) external pure returns (bytes32, uint256) {
        assembly {
            mstore(0x80, "a")
            mstore(0xa0, x)
            return(0x80, 64)
        }
    }
}
