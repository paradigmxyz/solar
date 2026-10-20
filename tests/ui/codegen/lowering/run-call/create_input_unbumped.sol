//@ codegen-matrix: standard
//@ run-call: plain => 0, 1, 2, 3, 7
//@ run-call: staticArguments => 96, 1, 2, 3, 11
//@ run-call: dynamicArguments => 192, 1, 2, 3, 99
//@ run-call: salted => 0, 1, 2, 3, 7
//@ run-call: repeated => 0, 7, 7

// The initcode of `new C(...)` is staged at the free memory pointer without reserving it, as solc
// does, and a constructor without arguments encodes nothing. Encoded arguments stay reserved. Memory
// below the pointer stays intact, and the constructor sees its arguments.
contract Plain {
    function value() external pure returns (uint256) {
        return 7;
    }
}

contract Arguments {
    uint256 public value;

    constructor(uint256 a, uint256 b) {
        value = a + b;
    }
}

contract Dynamic {
    uint256 public value;

    constructor(bytes memory data, uint256 extra) {
        value = data.length + extra;
    }
}

contract CreateInputUnbumped {
    function live() internal pure returns (uint256[] memory values) {
        values = new uint256[](3);
        values[0] = 1;
        values[1] = 2;
        values[2] = 3;
    }

    function fmp() internal pure returns (uint256 ptr) {
        assembly {
            ptr := mload(0x40)
        }
    }

    function plain() external returns (uint256 moved, uint256, uint256, uint256, uint256 value) {
        uint256[] memory values = live();
        uint256 before = fmp();
        Plain created = new Plain();
        moved = fmp() - before;
        return (moved, values[0], values[1], values[2], created.value());
    }

    function staticArguments() external returns (uint256 moved, uint256, uint256, uint256, uint256 value) {
        uint256[] memory values = live();
        uint256 before = fmp();
        Arguments created = new Arguments(5, 6);
        moved = fmp() - before;
        return (moved, values[0], values[1], values[2], created.value());
    }

    function dynamicArguments() external returns (uint256 moved, uint256, uint256, uint256, uint256 value) {
        uint256[] memory values = live();
        uint256 before = fmp();
        Dynamic created = new Dynamic("abc", 96);
        moved = fmp() - before;
        return (moved, values[0], values[1], values[2], created.value());
    }

    function salted() external returns (uint256 moved, uint256, uint256, uint256, uint256 value) {
        uint256[] memory values = live();
        uint256 before = fmp();
        Plain created = new Plain{salt: bytes32(uint256(1))}();
        moved = fmp() - before;
        return (moved, values[0], values[1], values[2], created.value());
    }

    function repeated() external returns (uint256 moved, uint256 first, uint256 second) {
        uint256 before = fmp();
        first = new Plain().value();
        second = new Plain().value();
        moved = fmp() - before;
    }
}
