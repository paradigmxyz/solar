// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface PureAddressProbe {
    function sender() external pure returns (address);
}

contract AddressContextDifferential {
    address internal constant IMPLEMENTATION =
        address(0x1000000000000000000000000000000000000001);

    function probe(uint256) external pure returns (address) {
        return PureAddressProbe(IMPLEMENTATION).sender();
    }

    fallback() external {
        assembly ("memory-safe") {
            mstore(0, caller())
            return(0, 32)
        }
    }
}
