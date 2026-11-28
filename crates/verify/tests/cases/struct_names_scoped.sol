pragma solidity ^0.8.20;
// Two declarations of a struct `Item` with different fields, in different interfaces: each is
// its own type, named `IOld.Item` and `INew.Item`.
interface IOld {
    struct Item {
        uint256 amount;
    }
    function take(Item calldata it) external;
}

interface INew {
    struct Item {
        address owner;
        uint8 kind;
    }
    function item() external view returns (Item memory);
}

contract Scoped {
    uint256 public total;

    /// @custom:ensures total == old(total) + v
    function ok_add(uint256 v) external {
        total += v;
    }

    /// @custom:ensures result == keccak256(abi.encode(it))
    function ok_hash(INew.Item memory it) public pure returns (bytes32 result) {
        result = keccak256(abi.encode(it));
    }

    // A hash is never 0, so this returns 1.
    /// @custom:ensures result == bytes32(0)
    function bad_through(address o) external pure returns (bytes32 result) {
        return ok_hash(INew.Item(o, 1)) == bytes32(0) ? bytes32(0) : bytes32(uint256(1));
    }

    function ok_call(INew c) external view returns (address) {
        return c.item().owner;
    }
}
