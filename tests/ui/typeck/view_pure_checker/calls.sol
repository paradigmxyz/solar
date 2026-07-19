type U is uint256;

using {add as +} for U global;

function add(U lhs, U rhs) pure returns (U) {
    return U.wrap(U.unwrap(lhs) + U.unwrap(rhs));
}

contract D {}

contract C {
    uint256[] values;

    function payableTarget() external payable {}

    function valueCall() external view {
        this.payableTarget{value: 1}();
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function create() external view {
        new D();
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function pushAndPop() external view {
        values.push();
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
        values.pop();
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function operator(U lhs, U rhs) external returns (U) {
        //~^ WARN: function state mutability can be restricted to pure
        return lhs + rhs;
    }
}
