pragma solidity ^0.8.20;
// A struct named like an enum or a contract elsewhere has an unknown layout: storage that reaches
// it is not verified, rather than read with the other declaration's size.
interface IE {
    enum Item {
        A,
        B
    }
}

interface IS {
    struct Item {
        uint256 a;
        uint256 b;
        uint256 c;
    }
}

contract EnumCollide {
    uint256 public total;
    IS.Item it;
    uint256 public after_;

    // `after_` is slot 4, which the assembly writes.
    /// @custom:ensures after_ == old(after_)
    function bad_clobber() external {
        assembly {
            sstore(4, 7)
        }
    }
}
