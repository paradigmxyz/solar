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
//@[none, gas, size] run-call: test_value() => 42
//@[none, gas, size] run-call: test_storage_view() => 0
//@[none, gas, size] run-call: test_storage_write() => 1
//@[none, gas, size] run-call: test_multi() => 7, 8
//@[none, gas, size] run-call: test_calldata [1, 2, 3] => 1
//@[none, gas, size] run-call: test_using() => 15

library Lib {
    struct S {
        uint a;
        bytes b;
    }

    function add(uint x, uint y) internal pure returns (uint) {
        return x + y;
    }
    function dup(bytes memory b) internal pure returns (bytes memory) {
        return abi.encodePacked(b, b);
    }
    function cat(string memory s, string memory t) internal pure returns (string memory) {
        return string(abi.encodePacked(s, t));
    }
    function get(S storage s) internal view returns (uint) {
        return s.a;
    }
    function bump(S storage s) internal {
        s.a += 1;
    }
    function pick(uint[] memory a) internal pure returns (uint, uint) {
        return (a[0], a[1]);
    }
    function first(uint[] calldata a) internal pure returns (uint) {
        return a[0];
    }
}

contract LinkedLibraryArgs {
    using Lib for *;
    Lib.S s;

    function test_value() external pure returns (uint) {
        return Lib.add(40, 2);
    }
    function test_storage_view() external view returns (uint) {
        return Lib.get(s);
    }
    function test_storage_write() external returns (uint) {
        Lib.bump(s);
        return s.a;
    }
    function test_multi() external pure returns (uint, uint) {
        uint[] memory a = new uint[](2);
        a[0] = 7;
        a[1] = 8;
        return Lib.pick(a);
    }
    function test_calldata(uint[] calldata a) external pure returns (uint) {
        return Lib.first(a);
    }
    function test_using() external pure returns (uint) {
        uint[] memory a = new uint[](2);
        a[0] = 7;
        a[1] = 8;
        (uint x, uint y) = a.pick();
        return x + y;
    }
}
