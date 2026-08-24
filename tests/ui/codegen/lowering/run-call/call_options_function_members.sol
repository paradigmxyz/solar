//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: callOptionMembers() => true
//@ run-call: addressSideEffects() => 123, true
//@ run-call: selectorSideEffects() => 123, true
// ported-from: test/libsolidity/semanticTests/functionTypes/stack_height_check_on_adding_gas_variable_to_function.sol

contract CallOptionMembers {
    uint256 private order;

    function g() external {}
    function h() external payable {}

    function select() internal returns (function() external payable) {
        order = order * 10 + 1;
        return this.h;
    }

    function option(uint256 marker) internal returns (uint256) {
        order = order * 10 + marker;
        return 0;
    }

    function callOptionMembers() external view returns (bool) {
        return this.g{gas: 42}.address == this.g.address &&
            this.g{gas: 42}.selector == this.g.selector &&
            this.h{gas: 42}.address == this.h.address &&
            this.h{gas: 42}.selector == this.h.selector &&
            this.h{gas: 42, value: 5}.address == this.h.address &&
            this.h{gas: 42, value: 5}.selector == this.h.selector;
    }

    function addressSideEffects() external returns (uint256, bool) {
        order = 0;
        address selected = select(){gas: option(2), value: option(3)}.address;
        return (order, selected == address(this));
    }

    function selectorSideEffects() external returns (uint256, bool) {
        order = 0;
        bytes4 selected = select(){value: option(2), gas: option(3)}.selector;
        return (order, selected == this.h.selector);
    }
}
