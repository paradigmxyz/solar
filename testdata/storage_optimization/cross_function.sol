// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @notice Test case for storage access with external calls
/// @dev External calls invalidate storage cache (re-entrancy safety)
///
/// Important: Storage cache must be invalidated after external calls
/// because the called contract may modify our storage (if delegatecall)
/// or call back into us (re-entrancy)

contract CrossFunction {
    uint256 public value;
    uint256 public counter;

    /// @dev Read before and after external call
    /// Cache must be invalidated after the call
    function readAroundCall(address target) external returns (uint256 before, uint256 after_) {
        before = value;  // First SLOAD

        // External call - invalidates cache
        (bool success,) = target.call("");
        require(success);

        after_ = value;  // Must SLOAD again - target may have called back
    }

    /// @dev Write before call - must flush before call
    function writeBeforeCall(address target, uint256 newValue) external {
        value = newValue;  // Must SSTORE before call

        // External call
        (bool success,) = target.call("");
        require(success);
    }

    /// @dev Multiple operations around call
    function complexAroundCall(address target) external {
        uint256 a = value;      // SLOAD
        uint256 b = value;      // Should reuse cache (before call)
        
        counter = a + b;        // SSTORE (before call, must happen)

        (bool success,) = target.call("");
        require(success);

        uint256 c = value;      // Must SLOAD again (cache invalidated)
        uint256 d = value;      // Should reuse cache (after call)
        
        counter = c + d;        // SSTORE
    }

    /// @dev Delegate call - especially dangerous for storage
    function delegateCallPattern(address impl) external {
        uint256 before = value;  // SLOAD

        // Delegate call - can modify our storage!
        (bool success,) = impl.delegatecall("");
        require(success);

        uint256 after_ = value;  // Must SLOAD again
        require(after_ == before, "Storage changed unexpectedly");
    }

    /// @dev Static call - theoretically safe but be conservative
    function staticCallPattern(address target) external view returns (uint256) {
        uint256 a = value;  // SLOAD

        // Static call - cannot modify state
        // But we're conservative and still invalidate cache
        (bool success, bytes memory data) = target.staticcall("");
        require(success);
        
        uint256 b = value;  // Could potentially reuse cache for staticcall
        return a + b + abi.decode(data, (uint256));
    }

    /// @dev Multiple calls - each invalidates cache
    function multipleExternalCalls(address t1, address t2) external {
        value = 1;  // SSTORE

        (bool s1,) = t1.call("");
        require(s1);

        value = 2;  // SSTORE (can't optimize with previous)

        (bool s2,) = t2.call("");
        require(s2);

        value = 3;  // SSTORE (can't optimize with previous)
    }

    /// @dev No external calls - full optimization possible
    function noExternalCalls() external {
        value = 1;  // Dead store
        value = 2;  // Dead store
        value = 3;  // Only this matters

        uint256 a = counter;    // SLOAD
        uint256 b = counter;    // Reuse cache
        uint256 c = counter;    // Reuse cache

        counter = a + b + c;    // SSTORE
    }
}
