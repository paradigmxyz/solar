// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.36;

contract Base {
    uint128 baseValue;
    uint64 transient baseTransient;
}

contract Right {
    uint64 rightValue;
    uint128 transient rightTransient;
}

contract Inheritance is Base, Right layout at 42 {
    uint constant IGNORED_CONSTANT = 1;
    uint immutable ignoredImmutable;
    uint64 derivedValue;
    uint64 transient derivedTransient;

    constructor() {
        ignoredImmutable = 1;
    }
}

contract Empty {}
