pragma solidity ^0.8.20;
// Two declarations of a struct `Item` with different fields: the name stands for no one
// datatype, so it stays opaque rather than declaring the datatype twice.
interface IOld { struct Item { uint256 amount; } function take(Item calldata it) external; }
interface INew { struct Item { address owner; uint8 kind; } }
struct Pair { uint256 a; uint256 b; }
contract Names {
    uint256 public total;
    Pair p;
    /// @custom:ensures total == old(total) + v
    function ok_add(uint256 v) external { total += v; }
    /// @custom:ensures total == old(total) + v
    function bad_add(uint256 v) external { total += v + 1; }
    /// structs with unique names keep their datatype
    /// @custom:ensures p.a == x && p.b == old(p.b)
    function ok_pair(uint256 x) external { p.a = x; }
    /// @custom:ensures total == old(total)
    function ok_forward(IOld o, IOld.Item calldata it) external view { it; o; }
}
contract Stored {
    // the slots an `Item` takes are not known, so neither are `after_`'s: a frontend error
    INew.Item it;
    uint256 public after_;
    function setAfter(uint256 v) external { after_ = v; }
}
