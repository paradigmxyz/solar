// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Boolean logic tests
/// @notice Tests for && and || operators with storage variables
contract BoolLogic {
    bool public flag1;
    bool public flag2;
    
    function setFlags(bool _f1, bool _f2) external {
        flag1 = _f1;
        flag2 = _f2;
    }
    
    function testAnd() external view returns (bool) {
        return flag1 && flag2;
    }
    
    function testOr() external view returns (bool) {
        return flag1 || flag2;
    }
    
    // Pure function tests (no storage interaction)
    function pureAnd(bool a, bool b) external pure returns (bool) {
        return a && b;
    }
    
    function pureOr(bool a, bool b) external pure returns (bool) {
        return a || b;
    }
}
