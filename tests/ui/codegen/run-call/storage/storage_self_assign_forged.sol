//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call-fail: bytesInvalidHeader() => Panic(0x22)
//@ run-call-fail: bytesCopyInvalidHeader() => Panic(0x22)
//@ run-call: bytesDirtyTail() => 0xaa00000000000000000000000000000000000000000000000000000000000000
//@ run-call: bytesCopyDirtyTail() => 0xaa00000000000000000000000000000000000000000000000000000000000000
//@ run-call: arrayDirtyHighByte() => 0
//@ run-call: structDirtyHighByte() => 0
//@ run-call: structCopyDirtyHighByte() => 0

// Assigning a storage value to itself loads it and writes it back, so assembly-forged storage is
// checked and cleaned: an invalid short `bytes` header panics with `Panic(0x22)` and dirty bits
// become zero. solc returns at once when the source and target slots are equal, leaving both as
// they were. A copy between distinct slots behaves the same in both compilers. See CODEGEN-032 in
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

    function structCopyDirtyHighByte() external returns (uint256 r) {
        s.a = 1;
        s.b = 2;
        assembly {
            sstore(s.slot, or(shl(248, 0xff), sload(s.slot)))
        }
        t = s;
        assembly {
            r := shr(248, sload(t.slot))
        }
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
