// SPDX-License-Identifier: MIT
pragma solidity ^0.8.10;

import {L2Encoder} from "aave-v3-core/contracts/misc/L2Encoder.sol";
import {IPool} from "aave-v3-core/contracts/interfaces/IPool.sol";
import {DataTypes} from "aave-v3-core/contracts/protocol/libraries/types/DataTypes.sol";

contract MockPool {
    function getReserveData(address asset) external pure returns (DataTypes.ReserveData memory data) {
        data.id = uint16(uint160(asset));
    }
}

contract L2EncoderHarness is L2Encoder {
    constructor() L2Encoder(IPool(address(new MockPool()))) {}
}
