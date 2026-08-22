//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call-fail: invalidShort() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022
//@ run-call-fail: invalidLong() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022
//@ run-call-fail: invalidShortDelete() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022
//@ run-call-fail: invalidLongDelete() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022

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
