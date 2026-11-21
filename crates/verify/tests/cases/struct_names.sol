pragma solidity ^0.8.20;
// Two declarations of a struct `Item` with different fields: structs are keyed by bare name,
// so the name stands for no one datatype and nothing in the file is verified.
interface IOld { struct Item { uint256 amount; } function take(Item calldata it) external; }
interface INew { struct Item { address owner; uint8 kind; } }
contract Names {
    uint256 public total;
    INew.Item it;
    uint256 public after_;
    /// @custom:ensures total == old(total) + v
    function add(uint256 v) external { total += v + 1; }
    /// @custom:ensures after_ == old(after_)
    function clobber() external { assembly { sstore(2, 7) } }
}
