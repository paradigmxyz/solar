// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice What the build is optimized for, as a constant.
/// @dev Compiler-owned module, imported as `solar:core/v1/Build.sol`.
/// `gasFirst()` is true when the compiler optimizes the build for runtime gas
/// and false when it optimizes for code size or not at all. A library can
/// guard a path that only pays for itself in gas with it: the path folds away
/// in the other builds. The guarded path must compute exactly what the code
/// after it computes; only the cost may differ. The body answers true, the
/// gas-first choice, for any other compiler.
library Build {
    /// @dev Whether the build optimizes for runtime gas.
    function gasFirst() internal pure returns (bool) {
        return true;
    }
}
