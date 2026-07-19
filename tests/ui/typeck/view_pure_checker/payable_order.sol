contract C {
    uint256 state;

    modifier readsValueAndWritesState() {
        msg.value;
        //~^ NOTE: `msg.value` or `callvalue()` appear here inside the modifier
        state = 1;
        _;
    }

    function internalValue() internal returns (uint256) {
        return msg.value;
    }

    function modified() public view readsValueAndWritesState {
        //~^ ERROR: this modifier uses `msg.value` or `callvalue()` and thus the function has to be payable or internal
    }
}
