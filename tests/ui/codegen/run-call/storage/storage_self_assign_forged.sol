//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: bytesInvalidHeader() => Panic(0x22)
//@ run-call-fail: bytesCopyInvalidHeader() => Panic(0x22)
//@ run-call: bytesDirtyTail() => 0xaa00000000000000000000000000000000000000000000000000000000000000
//@ run-call: bytesCopyDirtyTail() => 0xaa00000000000000000000000000000000000000000000000000000000000000
//@ run-call: bytesFreeMemoryGrowth() => 0x40
//@ run-call: staticSelfAssign() => false
//@ run-call: arrayDirtyHighByte() => 0
//@ run-call: structDirtyHighByte() => 0
//@ run-call: structCopyDirtyHighByte() => 0, 1, 2

// Assigning a storage value to itself loads it and writes it back, so assembly-forged storage is
// checked and cleaned: an invalid short `bytes` header panics with `Panic(0x22)` and dirty bits
// become zero. solc returns at once when the source and target slots are equal, leaving both as
// they were. A copy between distinct slots behaves the same in both compilers when the
// destination is clean; CODEGEN-025 covers a dirty one.
// With valid storage the result is the same, but the load advances the free memory pointer, which
// `bytesFreeMemoryGrowth()` shows; solc leaves it unchanged. The write-back also makes a static
// call fail, which `staticSelfAssign()` shows; solc writes nothing. See CODEGEN-035 in
// docs/SOLC_DIVERGENCE.md.
contract C {
    struct S {
        uint8 a;
        uint256 b;
    }

    bytes data;
    bytes other;
    uint8[] values;
    S s;
    S t;

    // CHECK-LABEL: fn @bytesInvalidHeader()
    // CHECK: icall @load_storage_bytes
    // CHECK: icall @store_storage_bytes
    function bytesInvalidHeader() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        data = data;
    }

    function bytesCopyInvalidHeader() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        other = data;
    }

    function bytesDirtyTail() external returns (bytes32 r) {
        data = _value();
        _dirtyTail();
        data = data;
        r = _tail();
    }

    function bytesCopyDirtyTail() external returns (bytes32 r) {
        data = _value();
        _dirtyTail();
        other = data;
        assembly {
            mstore(0, other.slot)
            r := sload(add(keccak256(0, 32), 2))
        }
    }

    function bytesFreeMemoryGrowth() external returns (uint256 growth) {
        data = hex"11";
        uint256 before;
        assembly {
            before := mload(0x40)
        }
        data = data;
        assembly {
            growth := sub(mload(0x40), before)
        }
    }

    function selfAssign() external {
        data = data;
    }

    function staticSelfAssign() external view returns (bool ok) {
        (ok, ) = address(this).staticcall(abi.encodeCall(this.selfAssign, ()));
    }

    function arrayDirtyHighByte() external returns (uint256 r) {
        values.push(1);
        values.push(2);
        assembly {
            mstore(0, values.slot)
            let slot := keccak256(0, 32)
            sstore(slot, or(shl(248, 0xff), sload(slot)))
        }
        values = values;
        assembly {
            mstore(0, values.slot)
            r := shr(248, sload(keccak256(0, 32)))
        }
    }

    function structDirtyHighByte() external returns (uint256 r) {
        s.a = 1;
        s.b = 2;
        assembly {
            sstore(s.slot, or(shl(248, 0xff), sload(s.slot)))
        }
        s = s;
        assembly {
            r := shr(248, sload(s.slot))
        }
    }

    function structCopyDirtyHighByte() external returns (uint256 r, uint256 a, uint256 b) {
        s.a = 1;
        s.b = 2;
        assembly {
            sstore(s.slot, or(shl(248, 0xff), sload(s.slot)))
        }
        t = s;
        assembly {
            r := shr(248, sload(t.slot))
        }
        a = t.a;
        b = t.b;
    }

    function _value() private pure returns (bytes memory m) {
        m = new bytes(65);
        m[64] = 0xaa;
    }

    // Sets the 31 padding bytes of the last data word of `data` to `0xff`.
    function _dirtyTail() private {
        assembly {
            mstore(0, data.slot)
            let slot := add(keccak256(0, 32), 2)
            sstore(slot, or(sload(slot), sub(shl(248, 1), 1)))
        }
    }

    function _tail() private view returns (bytes32 r) {
        assembly {
            mstore(0, data.slot)
            r := sload(add(keccak256(0, 32), 2))
        }
    }
}
