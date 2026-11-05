interface I {
    // solc also rejects `constructor();`, which is a parse error here.
    constructor() {}
    //~^ ERROR: functions in interfaces cannot have an implementation
    //~| ERROR: constructor cannot be defined in interfaces
}

// A constructor in an interface is never the missing visibility error, and never the one for a
// function that is not `external`.
interface J {
    constructor(uint256 x) {}
    //~^ ERROR: functions in interfaces cannot have an implementation
    //~| ERROR: constructor cannot be defined in interfaces
}

// Other contract kinds have their own errors, and a contract accepts a constructor.
library L {
    constructor() {} //~ ERROR: constructor cannot be defined in libraries
}

contract C {
    constructor() {}
}
