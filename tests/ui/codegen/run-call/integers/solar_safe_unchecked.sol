//@ codegen-matrix: standard
//@ run-call: increment 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: decrement 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: initialized() => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: divide 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: checkedIncrement 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: arrayAt 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

library Wrapping {
    function decrement(uint256 value) internal pure returns (uint256) {
        unchecked { return value - 1; }
    }
}

contract Base {
    uint256 internal seeded = Wrapping.decrement(0);
}

/// @custom:solar-safe
contract Test is Base {
    function increment(uint256 value) external pure returns (uint256) {
        unchecked { return value + 1; }
    }

    function decrement(uint256 value) external pure returns (uint256) {
        return Wrapping.decrement(value);
    }

    function initialized() external view returns (uint256) {
        return seeded;
    }

    function divide(uint256 value, uint256 divisor) external pure returns (uint256) {
        unchecked { return value / divisor; }
    }

    function checkedIncrement(uint256 value) external pure returns (uint256) {
        return value + 1;
    }

    function arrayAt(uint256 index) external pure returns (uint256) {
        uint256[] memory values = new uint256[](1);
        unchecked { return values[index]; }
    }
}
