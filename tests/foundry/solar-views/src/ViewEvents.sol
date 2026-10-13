// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

struct Order {
    uint256 id;
    bytes payload;
    uint64[] amounts;
}

/// Logs values that view decodes read in place.
contract ViewEvents {
    event Logged(bytes data, Order order, uint256[] words);
    event Indexed(bytes indexed tag, Order indexed order, uint256[] indexed words, string text);

    function emitCalldata(bytes calldata data) external {
        /// @custom:solar-view
        (bytes memory b, string memory s, uint256[] memory words, bytes[] memory items, Order memory order) =
            abi.decode(data, (bytes, string, uint256[], bytes[], Order));
        emit Logged(b, order, words);
        emit Indexed(items[0], order, words, s);
    }

    function emitMemory(bytes memory data) public {
        /// @custom:solar-view
        (bytes memory b, string memory s, uint256[] memory words, bytes[] memory items, Order memory order) =
            abi.decode(data, (bytes, string, uint256[], bytes[], Order));
        emit Logged(items[0], order, words);
        emit Indexed(b, order, words, s);
    }
}

/// The same events, logged from copies.
contract CopyEvents {
    event Logged(bytes data, Order order, uint256[] words);
    event Indexed(bytes indexed tag, Order indexed order, uint256[] indexed words, string text);

    function emitCalldata(bytes calldata data) external {
        (bytes memory b, string memory s, uint256[] memory words, bytes[] memory items, Order memory order) =
            abi.decode(data, (bytes, string, uint256[], bytes[], Order));
        emit Logged(b, order, words);
        emit Indexed(items[0], order, words, s);
    }

    function emitMemory(bytes memory data) public {
        (bytes memory b, string memory s, uint256[] memory words, bytes[] memory items, Order memory order) =
            abi.decode(data, (bytes, string, uint256[], bytes[], Order));
        emit Logged(items[0], order, words);
        emit Indexed(b, order, words, s);
    }
}
