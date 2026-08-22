//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none, gas, size] run-call-fail: invalidShort() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022
//@[none, gas, size] run-call-fail: invalidLong() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022
//@[none, gas, size] run-call-fail: invalidShortDelete() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022
//@[none, gas, size] run-call-fail: invalidLongDelete() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022

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
