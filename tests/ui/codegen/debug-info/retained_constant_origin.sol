//@ compile-flags: -Ogas --emit=srcmap -Zevm-ir-pipeline=none -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// The address mask is pushed once and kept on the stack for the six reads of `t`. The push is
// shared code, so it takes the merged location of those reads, one span per read.

// CHECK-LABEL: ; function go() external
// CHECK: push 0xffffffffffffffffffffffffffffffffffffffff !metadata(spans=[{{[0-9]+}}..{{[0-9]+}}, {{[0-9]+}}..{{[0-9]+}}, {{[0-9]+}}..{{[0-9]+}}, {{[0-9]+}}..{{[0-9]+}}, {{[0-9]+}}..{{[0-9]+}}, {{[0-9]+}}..{{[0-9]+}}])
interface T {
    function a() external;
    function b() external;
    function c() external;
    function d() external;
    function e() external;
    function f() external;
}

contract R {
    T t;

    function go() external {
        t.a();
        t.b();
        t.c();
        t.d();
        t.e();
        t.f();
    }
}
