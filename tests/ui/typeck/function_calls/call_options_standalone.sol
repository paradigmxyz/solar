// ported-from: test/libsolidity/syntaxTests/functionCalls/calloptions_repeated.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/external_functions_with_variable_number_of_stack_slots.sol

// Call options on an external function value are valid before a member access, where they have
// no effect, as in solc. They are still validated, and a function value carrying options cannot
// be used anywhere else, because dropping the options would silently change the call.

contract CallOptionMembers {
    function g() external {}
    function h() external payable {}
    function internalFn() internal {}

    function nested() external {
        this.h{gas: 42}{value: 5}(); //~ ERROR: function call options have already been set
        this.h{gas: 42}{value: 5}.address; //~ ERROR: function call options have already been set
    }

    function callOptionMembers() external returns (bool) {
        return this.g{gas: 42}.address == this.g.address &&
            this.g{gas: 42}.selector == this.g.selector &&
            this.h{gas: 42}.address == this.h.address &&
            this.h{gas: 42}.selector == this.h.selector &&
            this.h{gas: 42, value: 5}.address == this.h.address &&
            this.h{gas: 42, value: 5}.selector == this.h.selector;
    }

    function badOptions() external {
        this.g{value: 5}.address; //~ ERROR: cannot set option `value` on a non-payable function type
        this.h{gas: 1, gas: 2}.address; //~ ERROR: duplicate call option `gas`
        this.h{random: 1}.address; //~ ERROR: unknown call option `random`
        this.h{salt: bytes32(0)}.address; //~ ERROR: function call option `salt` can only be used with `new`
        internalFn{gas: 1}.selector; //~ ERROR: function call options can only be set on external function calls or contract creations
        //~^ ERROR: member `selector` not found on type `function ()`
    }

    // A function value carrying options is only usable for a call or a member access.
    function badPositions() external {
        function() external f = this.g{gas: 42}; //~ ERROR: call options must be part of a call expression
        f;
    }
}
