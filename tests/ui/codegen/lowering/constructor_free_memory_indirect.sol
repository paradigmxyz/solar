//@ codegen-matrix: standard
//@ run-call: ThroughBase::started => true
//@ run-call: ThroughModifier::started => true
//@ run-call: ThroughHelper::started => true

// An argument-free constructor that reads the free memory pointer only through
// a base constructor, a modifier, or an internal helper still starts the heap.
function freeMemoryStarted() pure returns (bool started) {
    assembly ("memory-safe") {
        started := iszero(lt(mload(0x40), 0x80))
    }
}

contract Base {
    bool public started;

    constructor() {
        started = freeMemoryStarted();
    }
}

contract ThroughBase is Base {
    uint256 public value;

    constructor() {
        value = 1;
    }
}

contract ThroughModifier {
    bool public started;

    modifier recordFreeMemory() {
        started = freeMemoryStarted();
        _;
    }

    constructor() recordFreeMemory {}
}

contract ThroughHelper {
    bool public started;

    constructor() {
        record();
    }

    function record() internal {
        started = freeMemoryStarted();
    }
}
