// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Child} from "../src/Child.sol";

contract ReuseTest {
    function testUsesCurrentArtifact() public {
        Child child = new Child(Child.Config(42, "cached"));
        require(child.number() == 42);
    }
}
