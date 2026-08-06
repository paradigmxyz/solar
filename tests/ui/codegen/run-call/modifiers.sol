//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: ModifierRuntime::runOne() => 123
//@ run-call: ModifierRuntime::runPublicTarget() => 123
//@ run-call: ModifierRuntime::runTwo() => 12345
//@ run-call: ModifierRuntime::runThree() => 1234567
//@ run-call: ModifierRuntime::runRepeated() => 12367
//@ run-call: ModifierRuntime::runArguments() => 123
//@ run-call: ModifierRuntime::runArgumentTiming() => 12345
//@ run-call: ModifierRuntime::runMutations() => 234
//@ run-call: ModifierRuntime::runMemoryArgument() => 23
//@ run-call: ModifierRuntime::runStorageArgument() => 12
//@ run-call: ModifierRuntime::runTwice() => 12325
//@ run-call: ModifierRuntime::runMaybe(bool) false => 13
//@ run-call: ModifierRuntime::runMaybe(bool) true => 123
//@ run-call: ModifierRuntime::runReturn() => 7, 19
//@ run-call: ModifierRuntime::runReturnPair() => 7, 8, 19
//@ run-call: ModifierRuntime::runMemoryReturn() => 0x22ae6da6b482f9b1b19b0b897c3fd43884180a1c5ee361e1107a1bc635649dda, 19
//@ run-call: ModifierRuntime::runPackedHashReturn() => 0xb10e2d527612073b26eecdfd717e6a320cf44b4afac2b0732d9fcbe2b7fa0cf6, 19
//@ run-call: ModifierRuntime::runSkip() => 1
//@ run-call: ModifierRuntime::runNestedSkip() => 19
//@ run-call-fail: ModifierRuntime::reject(uint256) 0
//@ run-call: ModifierRuntime::reject(uint256) 1 => 1
//@ run-call-fail: ModifierRuntime::returnThroughRejectingSuffix()
//@ run-call: ModifiedConstructor::trace(); constructor=[1] => 123
//@ run-call: ModifiedDerivedConstructor::trace() => 123
//@ run-call: DerivedModifier::run() => 327
//@ run-call: SpecialFunctionModifiers::runFallback() => 123
//@ run-call: SpecialFunctionModifiers::runReceive(); value=1 => 123

contract ModifierRuntime {
    uint256 private trace;
    uint256 private counter;
    uint256[] private values;

    modifier around(uint256 before_, uint256 after_) {
        mark(before_);
        _;
        mark(after_);
    }

    modifier capture(uint256 first, uint256 second) {
        mark(first);
        mark(second);
        _;
    }

    modifier prefix(uint256, uint256 digit) {
        mark(digit);
        _;
    }

    modifier mutate(uint256 value) {
        value++;
        mark(value);
        uint256 local = 3;
        local++;
        _;
        mark(local);
    }

    modifier memoryLength(bytes memory data) {
        mark(data.length);
        _;
    }

    modifier storageFirst(uint256[] storage data) {
        mark(data[0]);
        _;
    }

    modifier twice() {
        mark(1);
        _;
        mark(3);
        _;
        mark(5);
    }

    modifier maybe(bool run) {
        mark(1);
        if (run) {
            _;
        }
        mark(3);
    }

    modifier afterReturn() {
        _;
        mark(9);
    }

    modifier skip() {
        mark(1);
        return;
        _;
    }

    modifier nonzero(uint256 value) {
        require(value != 0);
        _;
    }

    modifier rejectingSuffix() {
        _;
        revert();
    }

    function mark(uint256 value) internal {
        trace = trace * 10 + value;
    }

    function next() internal returns (uint256) {
        counter++;
        return counter;
    }

    function markArgument(uint256 value) internal returns (uint256) {
        mark(value);
        return value;
    }

    function targetOne() internal around(1, 3) {
        mark(2);
    }

    function runOne() external returns (uint256) {
        targetOne();
        return trace;
    }

    function publicTarget() public around(1, 3) {
        mark(2);
    }

    function runPublicTarget() external returns (uint256) {
        publicTarget();
        return trace;
    }

    function targetTwo() internal around(1, 5) around(2, 4) {
        mark(3);
    }

    function runTwo() external returns (uint256) {
        targetTwo();
        return trace;
    }

    function targetThree() internal around(1, 7) around(2, 6) around(3, 5) {
        mark(4);
    }

    function runThree() external returns (uint256) {
        targetThree();
        return trace;
    }

    function targetRepeated() internal around(1, 7) around(2, 6) {
        mark(3);
    }

    function runRepeated() external returns (uint256) {
        targetRepeated();
        return trace;
    }

    function targetArguments() internal capture(next(), next()) {
        mark(3);
    }

    function runArguments() external returns (uint256) {
        targetArguments();
        return trace;
    }

    function targetArgumentTiming()
        internal
        prefix(markArgument(1), 2)
        prefix(markArgument(3), 4)
    {
        mark(5);
    }

    function runArgumentTiming() external returns (uint256) {
        targetArgumentTiming();
        return trace;
    }

    function targetMutations() internal mutate(1) {
        mark(3);
    }

    function runMutations() external returns (uint256) {
        targetMutations();
        return trace;
    }

    function targetMemoryArgument() internal memoryLength(hex"0102") {
        mark(3);
    }

    function runMemoryArgument() external returns (uint256) {
        targetMemoryArgument();
        return trace;
    }

    function targetStorageArgument() internal storageFirst(values) {
        mark(2);
    }

    function runStorageArgument() external returns (uint256) {
        values.push(1);
        targetStorageArgument();
        return trace;
    }

    function targetTwice() internal twice {
        mark(2);
    }

    function runTwice() external returns (uint256) {
        targetTwice();
        return trace;
    }

    function targetMaybe(bool run) internal maybe(run) {
        mark(2);
    }

    function runMaybe(bool run) external returns (uint256) {
        targetMaybe(run);
        return trace;
    }

    function targetReturn() internal afterReturn returns (uint256) {
        mark(1);
        return 7;
    }

    function runReturn() external returns (uint256, uint256) {
        uint256 value = targetReturn();
        return (value, trace);
    }

    function targetReturnPair() internal afterReturn returns (uint256, uint256) {
        mark(1);
        return (7, 8);
    }

    function runReturnPair() external returns (uint256, uint256, uint256) {
        (uint256 first, uint256 second) = targetReturnPair();
        return (first, second, trace);
    }

    function targetMemoryReturn() internal afterReturn returns (bytes memory) {
        mark(1);
        return hex"0102";
    }

    function runMemoryReturn() external returns (bytes32, uint256) {
        bytes memory result = targetMemoryReturn();
        return (keccak256(result), trace);
    }

    function targetPackedHashReturn() internal afterReturn returns (bytes32) {
        mark(1);
        bytes memory encoded = abi.encodePacked(uint256(1));
        return keccak256(encoded);
    }

    function runPackedHashReturn() external returns (bytes32, uint256) {
        bytes32 result = targetPackedHashReturn();
        return (result, trace);
    }

    function targetSkip() internal skip {
        mark(2);
    }

    function runSkip() external returns (uint256) {
        targetSkip();
        return trace;
    }

    function targetNestedSkip() internal afterReturn skip {
        mark(2);
    }

    function runNestedSkip() external returns (uint256) {
        targetNestedSkip();
        return trace;
    }

    function reject(uint256 value) external pure nonzero(value) returns (uint256) {
        return value;
    }

    function returnThroughRejectingSuffix() external pure rejectingSuffix returns (uint256) {
        return 1;
    }
}

contract ModifiedConstructor {
    uint256 public trace;

    modifier aroundConstruction(uint256 start) {
        trace = start;
        _;
        trace = trace * 10 + 3;
    }

    constructor(uint256 start) aroundConstruction(start) {
        trace = trace * 10 + 2;
    }
}

contract ModifiedBaseConstructor {
    uint256 public trace;

    modifier aroundConstruction(uint256 start) {
        trace = start;
        _;
        trace = trace * 10 + 3;
    }

    constructor(uint256 start) aroundConstruction(start) {
        trace = trace * 10 + 2;
    }
}

contract ModifiedDerivedConstructor is ModifiedBaseConstructor {
    constructor() ModifiedBaseConstructor(1) {}
}

contract BaseModifier {
    uint256 internal trace;

    modifier decorated() virtual {
        trace = 1;
        _;
        trace = trace * 10 + 9;
    }

    function target() internal decorated {
        trace = trace * 10 + 2;
    }
}

contract DerivedModifier is BaseModifier {
    modifier decorated() override {
        trace = 3;
        _;
        trace = trace * 10 + 7;
    }

    function run() external returns (uint256) {
        target();
        return trace;
    }
}

contract SpecialFunctionModifiers {
    uint256 private trace;

    modifier aroundCall() {
        trace = 1;
        _;
        trace = trace * 10 + 3;
    }

    fallback() external aroundCall {
        trace = trace * 10 + 2;
    }

    receive() external payable aroundCall {
        trace = trace * 10 + 2;
    }

    function runFallback() external returns (uint256) {
        (bool success,) = address(this).call(hex"deadbeef");
        require(success);
        return trace;
    }

    function runReceive() external payable returns (uint256) {
        (bool success,) = address(this).call{value: msg.value}("");
        require(success);
        return trace;
    }
}
