//@ codegen-matrix: standard
//@ compile-flags: --allow=5574 --allow=3860
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
// `bytes` and `string` values larger than 64 KiB do not fold, so doubling concatenations stay
// cheap to compile. Their uses call helpers that compute them at runtime.
bytes constant B0 = "0123456789abcdef0123456789abcdef";
bytes constant B1 = bytes.concat(B0, B0);
bytes constant B2 = bytes.concat(B1, B1);
bytes constant B3 = bytes.concat(B2, B2);
bytes constant B4 = bytes.concat(B3, B3);
bytes constant B5 = bytes.concat(B4, B4);
bytes constant B6 = bytes.concat(B5, B5);
bytes constant B7 = bytes.concat(B6, B6);
bytes constant B8 = bytes.concat(B7, B7);
bytes constant B9 = bytes.concat(B8, B8);
bytes constant B10 = bytes.concat(B9, B9);
bytes constant B11 = bytes.concat(B10, B10);
bytes constant B12 = bytes.concat(B11, B11);
string constant S = string.concat(string(B11), string(B11), "x");

// Building the values at runtime copies the folded `B11` into the code.
contract C {
    // CHECK-LABEL: fn @lengths()
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: {{v[0-9]+}} = icall @constant_
    // CHECK: icall @constant_
    // CHECK: insert_value struct0, undef struct0, 0, 0x10000
    function lengths() external pure returns (uint256, uint256, uint256) {
        return (B11.length, B12.length, bytes(S).length);
    }

    // The number of zero values in `new T[](n)` must not overflow while it is counted.
    function tooManyValues() external pure returns (uint256) {
        return new uint256[1][](2**63 + 1).length;
    }
}
