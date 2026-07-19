contract C {
    modifier readsValue() {
        msg.value;
        //~^ NOTE: `msg.value` or `callvalue()` appear here inside the modifier
        _;
    }

    constructor() readsValue {
        //~^ ERROR: this modifier uses `msg.value` or `callvalue()` and thus the constructor has to be payable
    }

    fallback() external {
        msg.value;
        //~^ ERROR: `msg.value` and `callvalue()` can only be used in payable public functions
    }

    receive() external payable {
        msg.value;
    }
}

library L {
    function value() public view returns (uint256) {
        return msg.value;
    }
}
