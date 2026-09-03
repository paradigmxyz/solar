//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call-fail: invalidShort => Panic(0x22)
//@ run-call-fail: invalidLong => Panic(0x22)
//@ run-call-fail: invalidShortDelete => Panic(0x22)
//@ run-call-fail: invalidLongDelete => Panic(0x22)
//@ run-call-fail: invalidShortLength => Panic(0x22)
//@ run-call-fail: invalidLongLength => Panic(0x22)
//@ run-call-fail: invalidShortIndex => Panic(0x22)
//@ run-call-fail: invalidLongIndex => Panic(0x22)
//@ run-call-fail: invalidShortIndexWrite => Panic(0x22)
//@ run-call-fail: invalidLongIndexWrite => Panic(0x22)
//@ run-call-fail: invalidShortIndexDelete => Panic(0x22)
//@ run-call-fail: invalidLongIndexDelete => Panic(0x22)
//@ run-call-fail: encodingCheckedBeforeBounds => Panic(0x22)
//@ run-call-fail: encodingCheckedBeforeBoundsWrite => Panic(0x22)

contract StorageBytesValidation {
    bytes private data;

    function invalidShort() external returns (bytes memory) {
        assembly {
            // A short encoding cannot claim 32 bytes.
            sstore(data.slot, 0x40)
        }
        return data;
    }

    function invalidLong() external returns (bytes memory) {
        assembly {
            // A long encoding must contain at least 32 bytes.
            sstore(data.slot, 0x21)
        }
        return data;
    }

    function invalidShortDelete() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        delete data;
    }

    function invalidLongDelete() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        delete data;
    }

    function invalidShortLength() external returns (uint256) {
        assembly {
            sstore(data.slot, 0x40)
        }
        return data.length;
    }

    function invalidLongLength() external returns (uint256) {
        assembly {
            sstore(data.slot, 0x21)
        }
        return data.length;
    }

    function invalidShortIndex() external returns (bytes1) {
        assembly {
            sstore(data.slot, 0x40)
        }
        return data[0];
    }

    function invalidLongIndex() external returns (bytes1) {
        assembly {
            sstore(data.slot, 0x21)
        }
        return data[0];
    }

    function invalidShortIndexWrite() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        data[0] = 0x01;
    }

    function invalidLongIndexWrite() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        data[0] = 0x01;
    }

    function invalidShortIndexDelete() external {
        assembly {
            sstore(data.slot, 0x40)
        }
        delete data[0];
    }

    function invalidLongIndexDelete() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        delete data[0];
    }

    // The encoding check runs before the bounds check, so an index past the
    // decoded length still reports the encoding panic and not `Panic(0x32)`.
    function encodingCheckedBeforeBounds() external returns (bytes1) {
        assembly {
            sstore(data.slot, 0x21)
        }
        return data[100];
    }

    function encodingCheckedBeforeBoundsWrite() external {
        assembly {
            sstore(data.slot, 0x21)
        }
        data[100] = 0x01;
    }
}
