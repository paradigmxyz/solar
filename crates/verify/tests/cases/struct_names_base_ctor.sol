pragma solidity ^0.8.20;
// A base constructor that reaches the ambiguous name `Item` (a struct and an enum share it) runs
// as part of the derived one, which never mentions it: the derived constructor is not verified
// either.
interface IOld {
    enum Item {
        A,
        B
    }
}

interface INew {
    struct Item {
        address owner;
        uint8 kind;
    }
}

contract Base {
    uint256 public h;

    constructor() {
        h = uint256(keccak256(abi.encode(INew.Item(address(1), 1))));
    }
}

contract Derived is Base {
    uint256 public total;

    /// @custom:ensures total == 1
    constructor() {
        total = 1;
    }

    /// @custom:ensures total == old(total) + v
    function ok_add(uint256 v) external {
        total += v;
    }
}
