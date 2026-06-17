// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/OverloadLib.sol";

contract OverloadLibTest {
    /// @notice Test single-arg find which calls two-arg overload
    function testFindSingleArg() public pure {
        uint256 result = OverloadLib.find(42);
        require(result == 42, "find(42) should return 42");
    }
    
    /// @notice Test two-arg find directly
    function testFindTwoArg() public pure {
        uint256 result = OverloadLib.find(42, true);
        require(result == 42, "find(42, true) should return 42");
        
        result = OverloadLib.find(42, false);
        require(result == 0, "find(42, false) should return 0");
    }
    
    /// @notice Test chained overload resolution
    function testFindDefault() public pure {
        uint256 result = OverloadLib.findDefault(100);
        require(result == 100, "findDefault(100) should return 100");
    }
}
