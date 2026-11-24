pragma solidity ^0.8.20;
// Two declarations of a struct `Item` with different fields: structs are keyed by bare name, so
// what reaches the name is not verified, but the rest of the file is.
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

    // expect: this function reaches `Item`
    /// @custom:ensures result == keccak256(abi.encode(it))
    function bad_hash(INew.Item memory it) public pure returns (bytes32 result) {
        result = keccak256(abi.encode(it));
    }

    // expect: this function reaches `Item`
    function bad_through(address o) external pure returns (bytes32) {
        return bad_hash(INew.Item(o, 1)) == bytes32(0) ? bytes32(0) : bytes32(uint256(1));
    }

    // expect: this function reaches `Item`
    function bad_call(INew c) external view returns (address) {
        return c.item().owner;
    }
}
