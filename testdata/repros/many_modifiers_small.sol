// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ManyModifiers {
    address public owner;
    uint256 public nonce;

    modifier mod0(uint256 val) {
        require(val > 0, "mod0");
        _;
    }

    modifier mod1(uint256 val) {
        require(val > 1, "mod1");
        _;
    }

    modifier mod2(uint256 val) {
        require(val > 2, "mod2");
        _;
    }

    modifier mod3(uint256 val) {
        require(val > 3, "mod3");
        _;
    }

    modifier mod4(uint256 val) {
        require(val > 4, "mod4");
        _;
    }

    modifier mod5(uint256 val) {
        require(val > 5, "mod5");
        _;
    }

    modifier mod6(uint256 val) {
        require(val > 6, "mod6");
        _;
    }

    modifier mod7(uint256 val) {
        require(val > 7, "mod7");
        _;
    }

    modifier mod8(uint256 val) {
        require(val > 8, "mod8");
        _;
    }

    modifier mod9(uint256 val) {
        require(val > 9, "mod9");
        _;
    }

    function multiModified(uint256 x) public
        mod0(x)
        mod1(x)
        mod2(x)
        mod3(x)
        mod4(x)
        mod5(x)
        mod6(x)
        mod7(x)
        mod8(x)
        mod9(x)
        returns (uint256)
    {
        return x;
    }
}
