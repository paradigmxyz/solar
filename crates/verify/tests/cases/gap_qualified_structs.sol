pragma solidity ^0.8.20;
// Structs of one name in different interfaces are different types, told apart by their
// qualified names; inside a contract that inherits one, the bare name is that one.
import {IEscrow, IEscrowV2} from "./auxiliary/gap_import_lib.sol";

contract Qualified {
    /// @custom:ensures result == a
    function ok_v1(uint256 a) external pure returns (uint256 result) {
        IEscrow.Deposit memory d = IEscrow.Deposit(a);
        result = d.amount;
    }

    /// @custom:ensures result == u
    function ok_v2(uint256 a, address u) external pure returns (address result) {
        IEscrowV2.Deposit memory d = IEscrowV2.Deposit(a, u);
        result = d.owner;
    }

    /// @custom:ensures result == a + 1
    function bad_v1(uint256 a) external pure returns (uint256 result) {
        IEscrow.Deposit memory d = IEscrow.Deposit(a);
        result = d.amount;
    }

    /// @custom:ensures result != keccak256(abi.encode(IEscrow.Deposit(a)))
    function bad_hash_differs(uint256 a) external pure returns (bytes32 result) {
        result = keccak256(abi.encode(IEscrow.Deposit(a)));
    }
}

contract Inherits is IEscrowV2 {
    /// @custom:ensures result == u
    function ok_bare(uint256 a, address u) external pure returns (address result) {
        Deposit memory d = Deposit(a, u);
        result = d.owner;
    }
}

// Each library's code reads its own `Item`, also when inlined into a contract that sees neither.
library L1 {
    struct Item {
        uint256 a;
    }

    function make(uint256 v) internal pure returns (uint256) {
        Item memory it = Item(v);
        return it.a;
    }
}

library L2 {
    struct Item {
        uint256 a;
        uint256 b;
    }

    function make(uint256 v) internal pure returns (uint256) {
        Item memory it = Item(v, v + 1);
        return it.b;
    }
}

contract UsesLibs {
    /// @custom:ensures r == v
    function ok_lib1(uint256 v) external pure returns (uint256 r) {
        r = L1.make(v);
    }

    /// @custom:ensures r == v + 1
    function ok_lib2(uint256 v) external pure returns (uint256 r) {
        r = L2.make(v);
    }

    /// @custom:ensures r == v
    function bad_lib2(uint256 v) external pure returns (uint256 r) {
        r = L2.make(v);
    }
}

// A contract's struct that shadows a file-level one of its name would name the contract's in a
// base's code verified in a derived contract: such names are still refused.
struct Top {
    uint256 z;
}

contract Shadows {
    struct Top {
        uint256 y;
        uint256 w;
    }

    // expect: Top
    /// @custom:ensures r == v
    function refused_shadowed(uint256 v) external pure returns (uint256 r) {
        Top memory t = Top(v, 0);
        r = t.y;
    }
}
