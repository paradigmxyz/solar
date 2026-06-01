// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {Test} from "forge-std/Test.sol";
import {StorageBench} from "../src/StorageBench.sol";

contract StorageBenchTest is Test {
    StorageBench bench;

    function setUp() public {
        bench = new StorageBench();
    }

    function testSimpleIncrement() public {
        assertEq(bench.counter(), 0);
        bench.simpleIncrement();
        assertEq(bench.counter(), 1);
        bench.simpleIncrement();
        assertEq(bench.counter(), 2);
    }

    function testReadModifyWrite() public {
        bench.readModifyWrite(10);
        assertEq(bench.counter(), 10);
        bench.readModifyWrite(5);
        assertEq(bench.counter(), 15);
    }

    function testMultipleReads() public {
        // Set counter first
        bench.simpleIncrement();
        bench.simpleIncrement();
        bench.simpleIncrement();  // counter = 3
        assertEq(bench.multipleReads(), 9);  // 3 + 3 + 3
    }

    function testMultipleWrites() public {
        bench.multipleWrites(1, 2, 3);
        assertEq(bench.counter(), 3);  // Only last write matters
    }

    function testBatchReads() public {
        bench.batchWrites(10, 20, 30);
        assertEq(bench.batchReads(), 60);
    }

    function testBatchWrites() public {
        bench.batchWrites(100, 200, 300);
        assertEq(bench.value1(), 100);
        assertEq(bench.value2(), 200);
        assertEq(bench.value3(), 300);
    }

    function testConditionalWrite() public {
        bench.conditionalWrite(42);
        assertEq(bench.counter(), 42);
        bench.conditionalWrite(42);  // No change
        assertEq(bench.counter(), 42);
        bench.conditionalWrite(100);
        assertEq(bench.counter(), 100);
    }

    function testSwapValues() public {
        bench.batchWrites(10, 20, 0);
        bench.swapValues();
        assertEq(bench.value1(), 20);
        assertEq(bench.value2(), 10);
    }

    function testPreIncrement() public {
        assertEq(bench.preIncrement(), 1);
        assertEq(bench.preIncrement(), 2);
        assertEq(bench.counter(), 2);
    }

    function testPostIncrement() public {
        assertEq(bench.postIncrement(), 0);
        assertEq(bench.postIncrement(), 1);
        assertEq(bench.counter(), 2);
    }

    function testCompoundAdd() public {
        bench.compoundAdd(5);
        assertEq(bench.counter(), 5);
        bench.compoundAdd(3);
        assertEq(bench.counter(), 8);
    }

    function testNonZeroWrite() public {
        bench.nonZeroWrite(42);
        assertEq(bench.counter(), 42);
        bench.nonZeroWrite(0);  // Should not write
        assertEq(bench.counter(), 42);
    }
}
