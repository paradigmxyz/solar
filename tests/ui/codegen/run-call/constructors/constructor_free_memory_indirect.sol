//@ codegen-matrix: standard
//@ run-call: ThroughBase::started => true
//@ run-call: ThroughModifier::started => true
//@ run-call: ThroughHelper::started => true
//@ run-call: Allocating::matches => true
//@ run-call: WithArgument::value; constructor=[7] => 7

// An argument-free constructor that reads the free memory pointer only through
// a base constructor, a modifier, or an internal helper still starts the heap,
// as do one that allocates directly and one that takes an argument.
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

contract Allocating {
    bytes32 public digest;

    constructor() {
        uint256[] memory words = new uint256[](3);
        uint256[] memory next = new uint256[](1);
        next[0] = 5;
        words[2] = 42;
        digest = keccak256(abi.encode(words));
    }

    function matches() external view returns (bool) {
        return digest == keccak256(abi.encode(uint256(0x20), uint256(3), uint256(0), uint256(0), uint256(42)));
    }
}

contract WithArgument {
    uint256 public value;

    constructor(uint256 v) {
        value = v;
    }
}
