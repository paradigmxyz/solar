//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: Locations::spill => 4, [9, 2, 3, 10]
//@ run-call: Locations::twoPushes => 4, [1, 2, 9, 10]
//@ run-call: Locations::indexOnce => 4, [9, 2, 3, 10], 1
//@ run-call: Locations::resetDuringTargets => 1, [9]
//@ run-call: Locations::popDuringTargets 2 => 3, [9, 2, 2]
//@ run-call: Locations::popDuringTargets 0 => 3, [9, 2, 0]
//@ run-call: Locations::emptyPushAfterPop => 3, [9, 2, 0]
//@ run-call: Functions::copy 1 => 99, 11, 11, 1
//@ run-call: Functions::copy 2 => 99, 11, 22, 2
//@ run-call: FunctionsOnly::copy => 11, 22
//@ run-call: FunctionsOnly::reverse => 11, 99

contract Locations {
    /// @custom:solar-storage inline
    uint64[] values;
    uint256 counter;

    // CHECK-LABEL: fn @spill(
    // CHECK: icall @inline_array_push_8_0
    // CHECK: sstore
    function spill() external returns (uint256, uint64[] memory) {
        values.push(1); values.push(2); values.push(3);
        (values[0], values.push()) = (9, 10);
        return (values.length, values);
    }

    function twoPushes() external returns (uint256, uint64[] memory) {
        values.push(1); values.push(2);
        (values.push(), values.push()) = (9, 10);
        return (values.length, values);
    }

    function resetIndex() internal returns (uint256) {
        delete values;
        values.push(1);
        return 0;
    }
    function resetDuringTargets() external returns (uint256, uint64[] memory) {
        values.push(1); values.push(2); values.push(3); values.push(4);
        (values[3], values[resetIndex()]) = (7, 9);
        return (values.length, values);
    }

    function popIndex() internal returns (uint256) {
        values.pop();
        return 0;
    }

    // A deferred write can reach the element a later target has already popped.
    function popDuringTargets(uint64 appended) external returns (uint256, uint64[] memory) {
        values.push(1); values.push(2); values.push(3);
        (values[2], values[popIndex()]) = (4, 9);
        values.push(appended);
        return (values.length, values);
    }

    function emptyPushAfterPop() external returns (uint256, uint64[] memory) {
        values.push(1); values.push(2); values.push(3);
        (values[2], values[popIndex()]) = (4, 9);
        values.push();
        return (values.length, values);
    }

    function index() internal returns (uint256) { counter += 1; return 0; }
    function indexOnce() external returns (uint256, uint64[] memory, uint256) {
        values.push(1); values.push(2); values.push(3);
        (values[index()], values.push()) = (9, 10);
        return (values.length, values, counter);
    }
}

contract Functions {
    /// @custom:solar-storage inline
    uint192[] numbers;
    /// @custom:solar-storage inline
    function() external view returns (uint256)[] callbacks;
    function first() external pure returns (uint256) { return 11; }
    function second() external pure returns (uint256) { return 22; }

    // CHECK-LABEL: fn @copy(
    // CHECK: icall @load_storage_inline_array_24_0,
    // CHECK: icall @load_storage_inline_array_24_0_external_function,
    function copy(uint256 n) external returns (uint192, uint256, uint256, uint256) {
        numbers.push(99);
        callbacks.push(this.first);
        if (n > 1) callbacks.push(this.second);
        uint192[] memory numeric = numbers;
        function() external view returns (uint256)[] memory functions = callbacks;
        return (numeric[0], functions[0](), functions[n - 1](), functions.length);
    }
}

contract FunctionsOnly {
    /// @custom:solar-storage inline
    function() external view returns (uint256)[] callbacks;
    /// @custom:solar-storage inline
    uint192[] numbers;
    function first() external pure returns (uint256) { return 11; }
    function second() external pure returns (uint256) { return 22; }
    function copy() external returns (uint256, uint256) {
        callbacks.push(this.first); callbacks.push(this.second);
        function() external view returns (uint256)[] memory functions = callbacks;
        return (functions[0](), functions[1]());
    }
    function reverse() external returns (uint256, uint192) {
        callbacks.push(this.first);
        numbers.push(99);
        function() external view returns (uint256)[] memory functions = callbacks;
        uint192[] memory numeric = numbers;
        return (functions[0](), numeric[0]);
    }
}
