// An internal function taking an address, whose mask the e-graph removes before
// `lower-integers` widens the argument, which every call then proves clean.
contract LateCseMasks {
    struct Entry {
        address owner;
        uint256 amount;
    }

    uint256 public total;

    function digest(address owner, uint256 amount) internal returns (bytes32) {
        Entry memory entry = Entry(owner, amount);
        bytes32 hash = keccak256(abi.encode(entry));
        total += amount;
        return hash;
    }

    function expected(address owner, uint256 amount) internal pure returns (bytes32) {
        return keccak256(abi.encode(uint256(uint160(owner)), amount));
    }

    function first(address owner, uint256 amount) external returns (bool) {
        return digest(owner, amount) == expected(owner, amount);
    }

    function second(address owner, uint256 amount) external returns (bool) {
        return digest(owner, amount + 1) == expected(owner, amount + 1);
    }
}
