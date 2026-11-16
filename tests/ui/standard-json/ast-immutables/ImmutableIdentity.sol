// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

contract ImmutableIdentity {
    address private immutable ORIGINAL;
    address private immutable LENDER;

    constructor(address lender) {
        ORIGINAL = address(this);
        LENDER = lender;
    }

    function values() external view returns (address, address) {
        return (ORIGINAL, LENDER);
    }
}
