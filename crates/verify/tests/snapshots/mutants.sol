pragma solidity ^0.8.20;
/// Every mutation operator, and the sites that are left alone.
/// @custom:invariant total <= 1000
contract Mutants {
    uint256 total;
    bool open;
    uint256[4] slots;

    modifier whenOpen() {
        if (!open) revert();
        _;
    }

    function deposit(uint256 x) external whenOpen {
        require(x > 0 && total + x <= 1000);
        total += x;
        slots[0] = x;
        if (x == 7) open = false;
        assembly {
            let y := add(x, 1)
        }
    }

    /// @custom:ensures result == (a > b || a == 0)
    function pick(uint256 a, uint256 b) internal pure returns (bool result) {
        result = a > b || a == 0;
    }

    /// Trusted: its body is not verified, so its sites are counted, not mutated.
    /// @custom:trusted
    /// @custom:ensures result >= a
    function bump(uint256 a) internal pure returns (uint256 result) {
        result = a + 1;
    }
}

/// No spec reaches this contract: its sites are counted, not mutated.
contract Plain {
    function double(uint256 a) external pure returns (uint256) {
        return a * 2;
    }
}
