// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract ManyEvents {
    event Event0(address indexed sender, uint256 indexed id, bytes32 data);
    event Event1(address indexed sender, uint256 indexed id, bytes32 data);
    event Event2(address indexed sender, uint256 indexed id, bytes32 data);
    event Event3(address indexed sender, uint256 indexed id, bytes32 data);
    event Event4(address indexed sender, uint256 indexed id, bytes32 data);
    event Event5(address indexed sender, uint256 indexed id, bytes32 data);
    event Event6(address indexed sender, uint256 indexed id, bytes32 data);
    event Event7(address indexed sender, uint256 indexed id, bytes32 data);
    event Event8(address indexed sender, uint256 indexed id, bytes32 data);
    event Event9(address indexed sender, uint256 indexed id, bytes32 data);

    function emitAll() public {
        emit Event0(msg.sender, 0, bytes32(uint256(0)));
        emit Event1(msg.sender, 1, bytes32(uint256(1)));
        emit Event2(msg.sender, 2, bytes32(uint256(2)));
        emit Event3(msg.sender, 3, bytes32(uint256(3)));
        emit Event4(msg.sender, 4, bytes32(uint256(4)));
        emit Event5(msg.sender, 5, bytes32(uint256(5)));
        emit Event6(msg.sender, 6, bytes32(uint256(6)));
        emit Event7(msg.sender, 7, bytes32(uint256(7)));
        emit Event8(msg.sender, 8, bytes32(uint256(8)));
        emit Event9(msg.sender, 9, bytes32(uint256(9)));
    }
}
