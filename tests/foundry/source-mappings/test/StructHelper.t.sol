// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {StructHelper} from "../src/StructHelper.sol";

contract StructHelperTest {
    function testSum() public {
        StructHelper helper = new StructHelper();
        assert(helper.sum(1, 2) == 5);
    }
}
