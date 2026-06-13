// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Multicall {
    struct Call {
        address target;
        bytes callData;
    }
    
    struct CallWithValue {
        address target;
        uint256 value;
        bytes callData;
    }
    
    struct Result {
        bool success;
        bytes returnData;
    }
    
    function aggregate(Call[] calldata calls) external returns (uint256 blockNumber, bytes[] memory returnData) {
        blockNumber = block.number;
        uint256 length = calls.length;
        returnData = new bytes[](length);
        
        for (uint256 i = 0; i < length; i++) {
            (bool success, bytes memory ret) = calls[i].target.call(calls[i].callData);
            require(success, "call failed");
            returnData[i] = ret;
        }
    }
    
    function tryAggregate(bool requireSuccess, Call[] calldata calls) external returns (Result[] memory results) {
        uint256 length = calls.length;
        results = new Result[](length);
        
        for (uint256 i = 0; i < length; i++) {
            (bool success, bytes memory ret) = calls[i].target.call(calls[i].callData);
            
            if (requireSuccess) {
                require(success, "call failed");
            }
            
            results[i] = Result(success, ret);
        }
    }
    
    function multicall(bytes[] calldata data) external returns (bytes[] memory results) {
        uint256 length = data.length;
        results = new bytes[](length);
        
        for (uint256 i = 0; i < length; i++) {
            (bool success, bytes memory result) = address(this).delegatecall(data[i]);
            require(success, "delegatecall failed");
            results[i] = result;
        }
    }
    
    function aggregateWithValue(CallWithValue[] calldata calls) external payable returns (bytes[] memory returnData) {
        uint256 length = calls.length;
        returnData = new bytes[](length);
        
        for (uint256 i = 0; i < length; i++) {
            (bool success, bytes memory ret) = calls[i].target.call{value: calls[i].value}(calls[i].callData);
            require(success, "call failed");
            returnData[i] = ret;
        }
    }
    
    function getBlockNumber() external view returns (uint256) {
        return block.number;
    }
    
    function getBlockHash(uint256 blockNumber) external view returns (bytes32) {
        return blockhash(blockNumber);
    }
    
    function getCurrentBlockTimestamp() external view returns (uint256) {
        return block.timestamp;
    }
    
    function getEthBalance(address addr) external view returns (uint256) {
        return addr.balance;
    }
}
