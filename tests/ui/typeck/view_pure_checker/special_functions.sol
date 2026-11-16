contract C {
    modifier readsValue() {
        msg.value; //~ WARN: statement has no effect
        //~^ NOTE: `msg.value` or `callvalue()` appear here inside the modifier
        _;
    }

    constructor() readsValue {
        //~^ ERROR: this modifier uses `msg.value` or `callvalue()` and thus the constructor has to be payable
    }

    fallback() external {
        msg.value; //~ WARN: statement has no effect
        //~^ ERROR: `msg.value` and `callvalue()` can only be used in payable public functions
    }

    receive() external payable {
        msg.value; //~ WARN: statement has no effect
    }
}

library L {
    function value() public view returns (uint256) {
        return msg.value;
    }
}
