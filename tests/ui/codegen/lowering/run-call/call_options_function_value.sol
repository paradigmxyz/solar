//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test_function => true
//@ run-call: sideEffects => 12
// ported-from: test/libsolidity/semanticTests/functionTypes/stack_height_check_on_adding_gas_variable_to_function.sol

contract CallOptionsFunctionValue {
    uint256 marker;

    function g() external {}
    function h() external payable {}

    function test_function() external view returns (bool) {
        assert(
            this.g.address == this.g.address &&
            this.g{gas: 42}.address == this.g.address &&
            this.g{gas: 42}.selector == this.g.selector
        );
        assert(
            this.h.address == this.h.address &&
            this.h{gas: 42}.address == this.h.address &&
            this.h{gas: 42}.selector == this.h.selector
        );
        assert(
            this.h{gas: 42, value: 5}.address == this.h.address &&
            this.h{gas: 42, value: 5}.selector == this.h.selector
        );
        return true;
    }

    function gasOpt() internal returns (uint256) {
        marker = marker * 10 + 1;
        return 42;
    }

    function valueOpt() internal returns (uint256) {
        marker = marker * 10 + 2;
        return 5;
    }

    // Discarded options are still evaluated, in order.
    function sideEffects() external returns (uint256) {
        this.h{gas: gasOpt(), value: valueOpt()}.address;
        return marker;
    }
}
