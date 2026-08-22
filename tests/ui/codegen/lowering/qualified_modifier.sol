//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: Derived::qualified() => 9, 2
//@ run-call: Derived::virtualDispatch() => 10, 1
// ported-from: test/libsolidity/semanticTests/modifiers/access_through_contract_name.sol

contract Base {
    uint256 internal value;

    modifier setValue() virtual {
        value = 2;
        _;
    }
}

contract Derived is Base {
    modifier setValue() override {
        value = 1;
        _;
    }

    function qualified() external Base.setValue returns (uint256, uint256) {
        return (9, value);
    }

    function virtualDispatch() external setValue returns (uint256, uint256) {
        return (10, value);
    }
}
