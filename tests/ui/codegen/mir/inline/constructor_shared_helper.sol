//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: f => 255, 3
//@ run-call: g => 2

// The constructor and `f` both store a bytes literal through one helper. Creation code carries
// its own copy of the helper, and the constructor is its only creation-side caller, so gas
// builds inline it there to keep its body out of the deployment, although the runtime calls it
// too.
contract ConstructorSharedHelper {
    struct S {
        bytes b;
    }

    S s;

    // CHECK-LABEL: fn @constructor()
    // CHECK-NOT: icall @store_storage_bytes_literal
    // CHECK-LABEL: fn @f()
    constructor() {
        s.b = hex"010203";
    }

    function f() external returns (uint256, uint256) {
        delete s;
        s.b = hex"010203";
        s.b[0] = bytes1(0xff);
        return (uint8(s.b[0]), s.b.length);
    }

    function g() external view returns (uint256) {
        return uint8(bytes(s.b)[1]);
    }
}
