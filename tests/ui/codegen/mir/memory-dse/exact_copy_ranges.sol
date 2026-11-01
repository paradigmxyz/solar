//@ codegen-matrix: standard
//@ run-call: copied 256, 123, 32 => 123
//@ run-call: copied 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe0, 789, 32 => 789
//@ run-call: zeroCopy 0 => 17
//@ run-call: unmatchedCount 0 => 7
//@ run-call: beforeCopy 5 => 7, 5
//@ run-call: partialCopy 5 => 261
//@ run-call: fromMemory 256 => 7
//@ run-call: throughHelper 0 => 7
//@ run-call: loopRead 1 => 7
//@ run-call: proxy 42, false => 42
//@ run-call-fail: proxy 42, true => 0x000000000000000000000000000000000000000000000000000000000000002a

contract Test {
    // A matching overwrite defines the returned word even when pointer arithmetic wraps.
    function copied(uint256 base, uint256, uint256 count) external pure returns (uint256) {
        assembly {
            let p := add(base, 0x20)
            mstore(0, 7)
            calldatacopy(p, 0x24, count)
            return(p, 0x20)
        }
    }

    // Zero-length accesses do not dereference even a maximal offset.
    function zeroCopy(uint256 count) external pure returns (uint256) {
        assembly {
            calldatacopy(not(0), not(0), count)
            mstore(0, 17)
            return(0, 0x20)
        }
    }

    // A shorter copy leaves the old word observable.
    function unmatchedCount(uint256 count) external pure returns (uint256) {
        assembly {
            mstore(0, 7)
            calldatacopy(0, 4, count)
            return(0, 0x20)
        }
    }

    // A read before the matching overwrite keeps the original stored value.
    function beforeCopy(uint256) external pure returns (uint256 oldValue, uint256 newValue) {
        assembly {
            mstore(0, 7)
            oldValue := mload(0)
            calldatacopy(0, 4, 0x20)
            newValue := mload(0)
        }
    }

    function partialCopy(uint256 value) external pure returns (uint256) {
        assembly {
            mstore(0, 0x107)
            mstore8(0x1f, value)
            return(0, 0x20)
        }
    }

    function fromMemory(uint256 destination) external pure returns (uint256) {
        assembly {
            mstore(0, 7)
            mcopy(destination, 0, 0x20)
            return(destination, 0x20)
        }
    }

    function throughHelper(uint256 pointer) external pure returns (uint256) {
        writeAndCopy(0, 0);
        assembly {
            return(pointer, 0x20)
        }
    }

    function writeAndCopy(uint256 pointer, uint256 count) private pure {
        assembly {
            mstore(0, 7)
            calldatacopy(pointer, 0, count)
        }
    }

    function loopRead(uint256 count) external pure returns (uint256) {
        assembly {
            mstore(0x20, 7)
            let pointer := 0
            for { let i := 0 } lt(i, count) { i := add(i, 1) } {
                calldatacopy(pointer, 4, 0x20)
                pointer := add(pointer, 0x20)
            }
            return(pointer, 0x20)
        }
    }

    // A real external call allocates and copies returndata before branching to return or revert.
    function proxy(uint256 value, bool fail) external view returns (uint256) {
        (bool success, bytes memory data) = address(this).staticcall(
            abi.encodeCall(this.echo, (value, fail))
        );
        assembly {
            let pointer := add(data, 0x20)
            let length := mload(data)
            switch success
            case 0 { revert(pointer, length) }
            default { return(pointer, length) }
        }
    }

    function echo(uint256 value, bool fail) external pure returns (uint256) {
        if (fail) {
            assembly {
                mstore(0, value)
                revert(0, 0x20)
            }
        }
        return value;
    }
}
