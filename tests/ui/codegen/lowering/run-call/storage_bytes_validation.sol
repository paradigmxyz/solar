//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call-fail: invalidShort => Panic(0x22)
//@ run-call-fail: invalidLong => Panic(0x22)
//@ run-call-fail: invalidShortDelete => Panic(0x22)
//@ run-call-fail: invalidLongDelete => Panic(0x22)

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
}
