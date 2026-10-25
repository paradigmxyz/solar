//@ codegen-matrix: standard
//@ run-call-fail: Deployer::createBase => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Deployer::createRaw => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
//@ run-call-fail: Deployer::createWide => 0x4e487b710000000000000000000000000000000000000000000000000000000000000021
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

// A raw enum immutable, whose runtime copy assembly reads, is range checked at the store too.
contract RawEnumImmutable {
    Choice internal immutable choice;

    constructor() {
        function(uint256) internal pure returns (uint256) typed = five;
        function(uint256) internal pure returns (Choice) retyped;
        assembly { retyped := typed }
        choice = retyped(0);
    }

    function raw() external view returns (uint256 word) {
        Choice copy = choice;
        assembly { word := copy }
    }
}

function wide(uint256) pure returns (uint256) {
    return 0x101;
}

// The callee's word has bits above the low byte, which the enum range check must still see.
contract WideEnumImmutable {
    Choice internal immutable choice;

    constructor() {
        function(uint256) internal pure returns (uint256) typed = wide;
        function(uint256) internal pure returns (Choice) retyped;
        assembly { retyped := typed }
        choice = retyped(0);
    }

    function read() external view returns (uint256) {
        return uint256(choice);
    }
}

function viaPointer() pure returns (Choice) {
    function(uint256) internal pure returns (uint256) typed = five;
    function(uint256) internal pure returns (Choice) retyped;
    assembly { retyped := typed }
    return retyped(0);
}

// The argument of a base constructor call reaches a raw enum immutable through a carrier cast, which
// must not drop the range check.
contract RawEnumBase {
    Choice internal immutable choice;

    constructor(Choice value) {
        choice = value;
    }

    function raw() external view returns (uint256 word) {
        Choice copy = choice;
        assembly { word := copy }
    }
}

contract RawEnumDerived is RawEnumBase(viaPointer()) {}

contract Deployer {
    function createBase() external {
        new RawEnumDerived();
    }

    function createWide() external {
        new WideEnumImmutable();
    }

    function createRaw() external {
        new RawEnumImmutable();
    }

    function create() external {
        new EnumImmutable();
    }
}
