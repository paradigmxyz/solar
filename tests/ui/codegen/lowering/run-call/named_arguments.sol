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
//@[none] run-call: b() => 123
//@[gas] run-call: b() => 123
//@[size] run-call: b() => 123
//@[none] run-call: c() => 123
//@[gas] run-call: c() => 123
//@[size] run-call: c() => 123
//@[none] run-call: d() => 7
//@[gas] run-call: d() => 7
//@[size] run-call: d() => 7
// ported-from: test/libsolidity/semanticTests/functionCall/named_args.sol

contract NamedArguments {
    function a(uint a, uint b, uint c) internal pure returns (uint r) {
        r = a * 100 + b * 10 + c;
    }
    function b() external pure returns (uint r) {
        r = a({a: 1, b: 2, c: 3});
    }
    function c() external pure returns (uint r) {
        r = a({b: 2, c: 3, a: 1});
    }

    modifier cap(uint x) {
        require(x > 0);
        _;
    }
    function d() external pure cap({x: 5}) returns (uint) {
        return a({c: 7, a: 0, b: 0});
    }
}
