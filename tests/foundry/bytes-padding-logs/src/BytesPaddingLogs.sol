// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// Each function emits a `bytes` object whose padding may be nonzero in memory: the object comes from
// dirty calldata, dirty returndata or `abi.encodePacked`, and free memory held junk when it was
// allocated. The event encoder and the indexed topic must read only its length.
contract BytesPaddingLogs {
    event Data(bytes data);
    event Topic(bytes indexed data);

    // Returns `n` bytes of `A` followed by 0xff padding.
    function dirty(uint256 n) external pure returns (bytes memory) {
        assembly {
            let rl := and(add(n, 31), not(31))
            mstore(0x00, 0x20)
            mstore(0x20, n)
            for { let i := 0 } lt(i, rl) { i := add(i, 32) } {
                mstore(add(0x40, i), 0x4141414141414141414141414141414141414141414141414141414141414141)
            }
            mstore(add(0x40, n), not(0))
            return(0x00, add(0x40, rl))
        }
    }

    function emitDecoded(bytes calldata encoded) external {
        require(_stir() != bytes32(0));
        bytes memory b = abi.decode(encoded, (bytes));
        emit Data(b);
        emit Topic(b);
    }

    function emitReturned(uint256 n) external {
        require(_stir() != bytes32(0));
        bytes memory b = this.dirty(n);
        emit Data(b);
        emit Topic(b);
    }

    function emitPacked(uint256 n) external {
        require(_stir() != bytes32(0));
        bytes memory data = new bytes(n - 1);
        for (uint256 i; i < data.length; ++i) data[i] = "A";
        bytes memory b = abi.encodePacked("A", data);
        emit Data(b);
        emit Topic(b);
    }

    // Leaves nonzero words in the free memory above the pointer.
    function _stir() private pure returns (bytes32) {
        uint256 m = ~msg.data.length;
        return keccak256(abi.encode(m, m, m, m, m, m, m, m));
    }
}
