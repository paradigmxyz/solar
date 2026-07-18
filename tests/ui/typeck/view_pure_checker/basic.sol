//@ compile-flags: -j8

contract Base {
    uint256 state;

    modifier readsState() {
        state;
        _;
    }

    modifier writesState() {
        state = 1;
        _;
    }
}

contract C is Base {
    function read() public pure returns (uint256) {
        return state;
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
    }

    function write() public view {
        state = 2;
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function calledView() internal view {}

    function callView() public pure {
        calledView();
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
    }

    function modifiedRead() public pure readsState {
        //~^ ERROR: function declared as pure, but this expression (potentially) reads from the environment or state and thus requires `view`
    }

    function modifiedWrite() public view writesState {
        //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
    }

    function value() public view returns (uint256) {
        return msg.value;
        //~^ ERROR: `msg.value` and `callvalue()` can only be used in payable public functions
    }

    function selector() public pure returns (bytes4) {
        return this.write.selector;
    }

    function assemblyWrite() public view {
        assembly {
            sstore(0, 1)
            //~^ ERROR: function cannot be declared as view because this expression (potentially) modifies the state
        }
    }

    function canBeView() public returns (uint256) {
        //~^ WARN: function state mutability can be restricted to view
        return state;
    }

    function canBePure(uint256 x) public returns (uint256) {
        //~^ WARN: function state mutability can be restricted to pure
        return x + 1;
    }
}
