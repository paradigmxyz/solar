pragma solidity ^0.8.20;
// Two structs that share a name and read alike are still distinct types: an overload on one is
// not an override of an overload on the other.
contract A {
    struct Item {
        uint256 amount;
    }
}

contract B {
    struct Item {
        uint256 amount;
    }
}

contract Parent {
    function pick(A.Item memory) internal pure returns (uint256) {
        return 1;
    }
}

contract Child is Parent {
    function pick(B.Item memory) internal pure returns (uint256) {
        return 2;
    }

    // Solidity calls the inherited `pick(A.Item)`, which returns 1.
    /// @custom:ensures result == 2
    function bad_alike_overload(A.Item calldata a) external pure returns (uint256 result) {
        return pick(a);
    }
}
