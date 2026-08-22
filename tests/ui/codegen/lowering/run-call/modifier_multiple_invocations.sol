//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: f 3 => 10
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
