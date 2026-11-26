pragma solidity ^0.8.20;

interface IEscrowV2 {
    struct Deposit {
        uint256 amount;
        address owner;
    }
}
