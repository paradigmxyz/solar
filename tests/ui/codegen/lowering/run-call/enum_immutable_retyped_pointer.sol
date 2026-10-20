//@ codegen-matrix: standard
//@ run-call-fail: Deployer::create => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021

// An enum immutable set from a call through an assembly-retyped pointer is range checked at the
// store, so deployment reverts with `Panic(0x21)` like solc.
enum Choice {
    First,
    Second
}

function five(uint256) pure returns (uint256) {
    return 5;
}

contract EnumImmutable {
    Choice internal immutable choice;

    constructor() {
        function(uint256) internal pure returns (uint256) typed = five;
        function(uint256) internal pure returns (Choice) retyped;
        assembly { retyped := typed }
        choice = retyped(0);
    }

    function read() external view returns (uint256) {
        return uint256(choice);
    }
}

contract Deployer {
    function create() external {
        new EnumImmutable();
    }
}
