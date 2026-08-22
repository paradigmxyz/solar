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
//@[none] run-call: high() => 0x00000000000000000000000000000000000000000000001122334455667788
//@[gas] run-call: high() => 0x00000000000000000000000000000000000000000000001122334455667788
//@[size] run-call: high() => 0x00000000000000000000000000000000000000000000001122334455667788
//@[none] run-call: arrayMatches() => true
//@[gas] run-call: arrayMatches() => true
//@[size] run-call: arrayMatches() => true

contract InternalFunctionPointerStoragePacking {
    function() internal fp;
    uint64 x;
    function() internal[4] fs;

    constructor() {
        fp = a;
        x = 0x1122334455667788;
        fs[0] = a;
        fs[1] = b;
        fs[2] = c;
        fs[3] = d;
    }

    function a() internal {}
    function b() internal {}
    function c() internal {}
    function d() internal {}

    function high() external view returns (uint64 value) {
        assembly {
            value := shr(64, sload(0))
        }
    }

    function arrayMatches() external view returns (bool) {
        assembly {
            let scalar := and(sload(0), 0xffffffffffffffff)
            let first := and(sload(1), 0xffffffffffffffff)
            mstore(0, eq(scalar, first))
            return(0, 32)
        }
    }
}
