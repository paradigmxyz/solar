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
//@[none, gas, size] run-call: readLength; constructor=["hello"] => 5
//@[none, gas, size] run-call: readBytes; constructor=["hello"] => 0x68656c6c6f
//@[none, gas, size] run-call: readBytes; constructor=["abcdefghijklmnopqrstuvwxyz0123456789"] => 0x6162636465666768696a6b6c6d6e6f707172737475767778797a30313233343536373839
contract StorageStringBytesConversion {
    string private stored;

    constructor(string memory input) {
        stored = input;
    }

    function readLength() external view returns (uint256) {
        return bytes(stored).length;
    }

    function readBytes() external view returns (bytes memory) {
        return bytes(stored);
    }
}
