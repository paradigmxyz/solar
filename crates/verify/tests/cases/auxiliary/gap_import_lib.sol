pragma solidity ^0.8.20;

library LibMath {
    function twice(uint256 x) internal pure returns (uint256) {
        return 2 * x;
    }
}

interface IEscrow {
    struct Deposit {
        uint256 amount;
    }
}

interface IEscrowV2 {
    struct Deposit {
        uint256 amount;
        address owner;
    }
}
