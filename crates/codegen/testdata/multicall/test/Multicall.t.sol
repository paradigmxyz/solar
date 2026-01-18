// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Multicall.sol";
import "../src/Counter.sol";

contract MulticallTest {
    Multicall multicall;
    Counter counter;
    
    function setUp() public {
        multicall = new Multicall();
        counter = new Counter();
    }
    
    function testAggregateSingleCall() public {
        Multicall.Call[] memory calls = new Multicall.Call[](1);
        calls[0] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("increment()")
        });
        
        (, bytes[] memory results) = multicall.aggregate(calls);
        
        assert(results.length == 1);
        assert(counter.count() == 1);
    }
    
    function testAggregateMultipleCalls() public {
        Multicall.Call[] memory calls = new Multicall.Call[](3);
        
        calls[0] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("increment()")
        });
        calls[1] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("increment()")
        });
        calls[2] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("increment()")
        });
        
        multicall.aggregate(calls);
        assert(counter.count() == 3);
    }
    
    function testAggregateReturnsData() public {
        counter.setCount(10);
        
        Multicall.Call[] memory calls = new Multicall.Call[](1);
        calls[0] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("getCount()")
        });
        
        (, bytes[] memory results) = multicall.aggregate(calls);
        
        uint256 result = abi.decode(results[0], (uint256));
        assert(result == 10);
    }
    
    function testAggregateReturnsBlockNumber() public {
        Multicall.Call[] memory calls = new Multicall.Call[](1);
        calls[0] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("getCount()")
        });
        
        (uint256 blockNumber,) = multicall.aggregate(calls);
        assert(blockNumber == block.number);
    }
    
    function testTryAggregateAllSuccess() public {
        Multicall.Call[] memory calls = new Multicall.Call[](2);
        calls[0] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("increment()")
        });
        calls[1] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("increment()")
        });
        
        Multicall.Result[] memory results = multicall.tryAggregate(false, calls);
        
        assert(results.length == 2);
        assert(results[0].success);
        assert(results[1].success);
    }
    
    function testTryAggregatePartialSuccess() public {
        Multicall.Call[] memory calls = new Multicall.Call[](2);
        calls[0] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("getCount()")
        });
        calls[1] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("increment()")
        });
        
        Multicall.Result[] memory results = multicall.tryAggregate(false, calls);
        
        assert(results.length == 2);
        assert(results[0].success);
        assert(results[1].success);
        assert(counter.count() == 1);
    }
    
    function testGetBlockNumber() public view {
        assert(multicall.getBlockNumber() == block.number);
    }
    
    function testGetCurrentBlockTimestamp() public view {
        assert(multicall.getCurrentBlockTimestamp() == block.timestamp);
    }
    
    function testGetEthBalance() public view {
        assert(multicall.getEthBalance(address(this)) == address(this).balance);
    }
    
    function testMixedOperations() public {
        counter.setCount(5);
        
        Multicall.Call[] memory calls = new Multicall.Call[](4);
        
        calls[0] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("getCount()")
        });
        calls[1] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("increment()")
        });
        calls[2] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("getCount()")
        });
        calls[3] = Multicall.Call({
            target: address(counter),
            callData: abi.encodeWithSignature("add(uint256,uint256)", uint256(10), uint256(20))
        });
        
        (, bytes[] memory results) = multicall.aggregate(calls);
        
        uint256 firstCount = abi.decode(results[0], (uint256));
        uint256 secondCount = abi.decode(results[2], (uint256));
        uint256 sum = abi.decode(results[3], (uint256));
        
        assert(firstCount == 5);
        assert(secondCount == 6);
        assert(sum == 30);
    }
    
    function testEmptyAggregate() public {
        Multicall.Call[] memory calls = new Multicall.Call[](0);
        
        (uint256 blockNumber, bytes[] memory results) = multicall.aggregate(calls);
        
        assert(blockNumber == block.number);
        assert(results.length == 0);
    }
}
