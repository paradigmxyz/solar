//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: f 3 => 10
// ported-from: test/libsolidity/semanticTests/modifiers/function_modifier_multiple_times_local_vars.sol

contract ModifierMultipleInvocations {
    uint256 private a;

    modifier addAndRestore(uint256 value) {
        uint256 local = value;
        a += local;
        _;
        a -= local;
        assert(local == value);
    }

    function f(uint256 value)
        external
        addAndRestore(2)
        addAndRestore(5)
        addAndRestore(value)
        returns (uint256)
    {
        return a;
    }
}
