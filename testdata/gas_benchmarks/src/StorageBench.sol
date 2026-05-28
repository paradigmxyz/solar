// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Storage Operations Benchmark
/// @notice Tests storage optimization patterns
contract StorageBench {
    uint256 public counter;
    uint256 public value1;
    uint256 public value2;
    uint256 public value3;

    /// @notice Single storage read and write
    function simpleIncrement() public {
        counter = counter + 1;
    }

    /// @notice Read-modify-write pattern
    function readModifyWrite(uint256 delta) public {
        uint256 current = counter;
        counter = current + delta;
    }

    /// @notice Multiple reads from same slot (CSE opportunity)
    function multipleReads() public view returns (uint256) {
        return counter + counter + counter;
    }

    /// @notice Multiple writes to same slot (only last matters)
    function multipleWrites(uint256 a, uint256 b, uint256 c) public {
        counter = a;
        counter = b;
        counter = c;  // Only this write matters
    }

    /// @notice Batch storage reads
    function batchReads() public view returns (uint256) {
        return value1 + value2 + value3;
    }

    /// @notice Batch storage writes
    function batchWrites(uint256 a, uint256 b, uint256 c) public {
        value1 = a;
        value2 = b;
        value3 = c;
    }

    /// @notice Conditional storage write
    function conditionalWrite(uint256 newValue) public {
        if (counter != newValue) {
            counter = newValue;
        }
    }

    /// @notice Swap values (requires temp)
    function swapValues() public {
        uint256 temp = value1;
        value1 = value2;
        value2 = temp;
    }

    /// @notice Pre-increment pattern
    function preIncrement() public returns (uint256) {
        counter = counter + 1;
        return counter;
    }

    /// @notice Post-increment pattern
    function postIncrement() public returns (uint256) {
        uint256 old = counter;
        counter = counter + 1;
        return old;
    }

    /// @notice Compound assignment
    function compoundAdd(uint256 delta) public {
        counter += delta;
    }

    /// @notice Zero check before write
    function nonZeroWrite(uint256 val) public {
        if (val != 0) {
            counter = val;
        }
    }
}
