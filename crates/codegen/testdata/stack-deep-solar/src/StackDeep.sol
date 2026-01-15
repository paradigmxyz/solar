// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Tests Solar's ability to handle >16 local variables (stack too deep for solc).
/// @dev This contract cannot be compiled by solc due to stack depth limitations.
contract StackDeep {
    /// @notice Function with 20 local variables - exceeds EVM's 16-slot stack limit.
    function manyLocals(uint256 a) external pure returns (uint256) {
        uint256 v1 = a + 1;
        uint256 v2 = v1 + 1;
        uint256 v3 = v2 + 1;
        uint256 v4 = v3 + 1;
        uint256 v5 = v4 + 1;
        uint256 v6 = v5 + 1;
        uint256 v7 = v6 + 1;
        uint256 v8 = v7 + 1;
        uint256 v9 = v8 + 1;
        uint256 v10 = v9 + 1;
        uint256 v11 = v10 + 1;
        uint256 v12 = v11 + 1;
        uint256 v13 = v12 + 1;
        uint256 v14 = v13 + 1;
        uint256 v15 = v14 + 1;
        uint256 v16 = v15 + 1;
        uint256 v17 = v16 + 1;
        uint256 v18 = v17 + 1;
        uint256 v19 = v18 + 1;
        uint256 v20 = v19 + 1;
        // Use all variables to prevent optimization
        return v1 + v2 + v3 + v4 + v5 + v6 + v7 + v8 + v9 + v10
             + v11 + v12 + v13 + v14 + v15 + v16 + v17 + v18 + v19 + v20;
    }

    /// @notice Function with many parameters AND many locals.
    function manyParamsAndLocals(
        uint256 a, uint256 b, uint256 c, uint256 d,
        uint256 e, uint256 f, uint256 g, uint256 h
    ) external pure returns (uint256) {
        uint256 v1 = a + b;
        uint256 v2 = c + d;
        uint256 v3 = e + f;
        uint256 v4 = g + h;
        uint256 v5 = v1 + v2;
        uint256 v6 = v3 + v4;
        uint256 v7 = v5 + v6;
        uint256 v8 = v7 + a;
        uint256 v9 = v8 + b;
        uint256 v10 = v9 + c;
        uint256 v11 = v10 + d;
        uint256 v12 = v11 + e;
        uint256 v13 = v12 + f;
        uint256 v14 = v13 + g;
        uint256 v15 = v14 + h;
        // 8 params + 15 locals = 23 active variables
        return v1 + v2 + v3 + v4 + v5 + v6 + v7 + v8 + v9 + v10
             + v11 + v12 + v13 + v14 + v15;
    }
}
