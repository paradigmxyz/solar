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
//@[none, gas, size] run-call: h(); value=1 => 1
// ported-from: test/libsolidity/semanticTests/functionTypes/inline_array_with_value_call_option.sol

contract FunctionPointerInlineArrayOptions {
    function f() external payable returns (uint256) {
        assert(msg.value > 0);
        return 1;
    }

    function g() external payable returns (uint256) {
        assert(msg.value > 0);
        return 2;
    }

    function h() public payable returns (uint256) {
        return [this.f, this.g][0]{value: 1}();
    }
}
