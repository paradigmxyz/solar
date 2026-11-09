//@ run-call: Emitter::emitThenAllocate 7 => 7
//@ run-call: Emitter::emitThenCall 9 => 9

// A static event payload wider than the two scratch words must not encode at
// address zero: that would overwrite the free-memory pointer and the zero
// slot, and the next allocation would explode off the poisoned pointer.

interface ISink {
    function sink(uint256 value) external pure returns (uint256);
}

contract Emitter {
    event Wide(
        address indexed who,
        address loanToken,
        address collateralToken,
        address oracle,
        address irm,
        uint256 lltv
    );

    function emitThenAllocate(uint256 value) external returns (uint256) {
        emit Wide(
            msg.sender,
            address(1),
            address(2),
            address(3),
            address(4),
            type(uint256).max
        );
        // Allocates off the free-memory pointer the emit must not clobber.
        uint256[] memory scratch = new uint256[](4);
        scratch[0] = value;
        return scratch[0];
    }

    function emitThenCall(uint256 value) external returns (uint256) {
        emit Wide(
            msg.sender,
            address(5),
            address(6),
            address(7),
            address(8),
            type(uint256).max
        );
        // The external self-call ABI-encodes through fresh memory.
        return ISink(address(this)).sink(value);
    }

    function sink(uint256 value) external pure returns (uint256) {
        return value;
    }
}
