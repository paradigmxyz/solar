pragma solidity ^0.8.20;

import {SHARED} from "./K1.sol";

contract UsesK1 {
    function k1() internal pure returns (uint256) {
        return SHARED;
    }
}
