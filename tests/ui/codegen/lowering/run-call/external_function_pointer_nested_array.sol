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
//@[none] run-call: test() => 5, 6, 7
//@[gas] run-call: test() => 5, 6, 7
//@[size] run-call: test() => 5, 6, 7
// ported-from: test/libsolidity/semanticTests/array/function_array_cross_calls.sol

contract ExternalFunctionPointerNestedArrayTarget {
    function f(
        function() external returns (function() external returns (uint256))[] memory x
    )
        public
        returns (function() external returns (uint256)[3] memory r)
    {
        r[0] = x[0]();
        r[1] = x[1]();
        r[2] = x[2]();
    }
}

contract ExternalFunctionPointerNestedArray {
    uint256 counter;

    function test() public returns (uint256, uint256, uint256) {
        function() external returns (function() external returns (uint256))[] memory x =
            new function() external returns (function() external returns (uint256))[](3);
        for (uint256 i = 0; i < x.length; i++) x[i] = this.h;
        x[0] = this.htwo;
        function() external returns (uint256)[3] memory y =
            (new ExternalFunctionPointerNestedArrayTarget()).f(x);
        return (y[0](), y[1](), y[2]());
    }

    function e() public returns (uint256) {
        //~^ WARN: function state mutability can be restricted to pure
        return 5;
    }

    function f() public returns (uint256) {
        //~^ WARN: function state mutability can be restricted to pure
        return 6;
    }

    function g() public returns (uint256) {
        //~^ WARN: function state mutability can be restricted to pure
        return 7;
    }

    function h() public returns (function() external returns (uint256)) {
        return counter++ == 0 ? this.f : this.g;
    }

    function htwo() public returns (function() external returns (uint256)) {
        //~^ WARN: function state mutability can be restricted to view
        return this.e;
    }
}
