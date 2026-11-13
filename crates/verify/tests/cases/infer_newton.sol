// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Loop invariants inferred for loops without a counter. Every ok_* must verify, every bad_* must
// fail. No loop here carries a hand-written invariant.
contract InferNewton {
    // Math.sqrt (Babylonian method): the loop runs while the next estimate improves.
    /// @custom:ensures z * z <= y && y < (z + 1) * (z + 1)
    function ok_sqrt(uint256 y) public pure returns (uint256 z) {
        if (y > 3) {
            z = y;
            uint256 x = y / 2 + 1;
            while (x < z) {
                z = x;
                x = (y / x + x) / 2;
            }
        } else if (y != 0) {
            z = 1;
        }
    }

    // A wrong postcondition: perfect squares are not strictly above z * z.
    // expect: postcondition not satisfied
    /// @custom:ensures z * z < y && y < (z + 1) * (z + 1)
    function bad_sqrt_strict(uint256 y) public pure returns (uint256 z) {
        if (y > 3) {
            z = y;
            uint256 x = y / 2 + 1;
            while (x < z) {
                z = x;
                x = (y / x + x) / 2;
            }
        } else if (y != 0) {
            z = 1;
        }
    }

    // A wrong result for small inputs.
    // expect: postcondition not satisfied
    /// @custom:ensures z * z <= y && y < (z + 1) * (z + 1)
    function bad_sqrt_small(uint256 y) public pure returns (uint256 z) {
        if (y > 3) {
            z = y;
            uint256 x = y / 2 + 1;
            while (x < z) {
                z = x;
                x = (y / x + x) / 2;
            }
        } else if (y != 0) {
            z = 2;
        }
    }

    // The update skips the halving: the estimate never settles on the root.
    // expect: postcondition not satisfied
    /// @custom:ensures z * z <= y && y < (z + 1) * (z + 1)
    function bad_sqrt_update(uint256 y) public pure returns (uint256 z) {
        if (y > 3) {
            z = y;
            uint256 x = y / 2 + 1;
            while (x < z) {
                z = x;
                x = y / x;
            }
        } else if (y != 0) {
            z = 1;
        }
    }
}
