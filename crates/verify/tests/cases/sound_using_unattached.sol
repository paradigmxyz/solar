pragma solidity ^0.8.20;
// A library has `pop(uint256[] storage)` that does nothing, but no `using` attaches it:
// `arr.pop()` is the builtin pop, which shrinks the array.
library Stacks {
    function pop(uint256[] storage) internal pure {}
}

contract BuiltinPop {
    uint256[] internal arr;

    /// @custom:requires arr.length > 0
    /// @custom:ensures arr.length == old(arr.length)
    function bad_pop() external {
        arr.pop();
    }

    /// @custom:requires arr.length > 0
    /// @custom:ensures arr.length == old(arr.length) - 1
    function ok_pop() external {
        arr.pop();
    }
}
