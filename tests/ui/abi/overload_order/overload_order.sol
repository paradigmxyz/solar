//@ compile-flags: --emit=abi
//@ filecheck:

// Overloads are listed in ascending selector order, like solc.
// CHECK-LABEL: overload_order.sol:C"
// CHECK-SAME: "name":"base"
// CHECK-SAME: "name":"flag"
// CHECK-SAME: "name":"word"
// CHECK-SAME: "name":"st"
// CHECK-SAME: "name":"enm"
// CHECK-SAME: "name":"udvt"
// CHECK-SAME: "name":"addr"
// CHECK-SAME: "name":"one"
// CHECK-SAME: "name":"three"
// CHECK-SAME: "name":"two"

// Library selectors use the struct and enum names, which changes the order.
// CHECK-LABEL: overload_order.sol:L"
// CHECK-SAME: "name":"enm"
// CHECK-SAME: "name":"udvt"
// CHECK-SAME: "name":"addr"
// CHECK-SAME: "name":"st"

struct S {
    uint256 a;
}

enum E {
    A
}

type U is uint64;

contract Base {
    // 0x13d1aa2e
    function f(uint256 base, uint256) external {}
}

contract C is Base {
    // 0x29e99f07
    function test(uint256 one) external {}
    // 0xeb8ac921
    function test(uint256 two, uint256) external {}
    // 0x61805cc3
    function test(uint256 three, uint256, uint256) external {}

    // 0xb3de648b
    function f(uint256 word) external {}
    // 0x98c3a6c1
    function f(bool flag) external {}

    // 0xcadaacbf
    function g(address addr) external {}
    // 0xb3e8e519
    function g(U udvt) external {}
    // 0xab088fbd
    function g(E enm) external {}
    // 0x032abe90
    function g(S memory st) external {}
}

library L {
    // 0xaf412119
    function h(address addr) external view {}
    // 0x879b2163
    function h(U udvt) external view {}
    // 0x66086fad
    function h(E enm) external view {}
    // 0xe3568738
    function h(S memory st) external view {}
}
