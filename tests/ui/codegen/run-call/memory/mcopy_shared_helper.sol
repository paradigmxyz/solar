//@ compile-flags: --evm-version paris
//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: join(bytes,bytes) 0x0102, 0x030405 => 0x0102030405
//@ run-call: join(bytes,bytes) 0x, 0x0102030405060708091011121314151617181920212223242526272829303132333435 => 0x0102030405060708091011121314151617181920212223242526272829303132333435
//@ run-call: twice(bytes) 0x0102030405060708091011121314151617181920212223242526272829303132333435 => 0x01020304050607080910111213141516171819202122232425262728293031323334350102030405060708091011121314151617181920212223242526272829303132333435
//@ run-call: twice(bytes) 0x => 0x

// Without `MCOPY` every memory copy is a word loop with a partial-word tail. With more than
// one copy site the size objective builds the loop once as `mcopy_words` and calls it, while
// the gas objective keeps every loop in place; both must copy exactly `len` bytes.
contract MCopySharedHelper {
    function join(bytes memory a, bytes memory b) external pure returns (bytes memory) {
        return abi.encodePacked(a, b);
    }

    function twice(bytes memory a) external pure returns (bytes memory) {
        return bytes.concat(a, a);
    }
}
