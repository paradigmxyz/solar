//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: B::nonEmptyCode() => true
//@ run-call: EmptyContractRuntime::hasNonCallableRuntime() => true
//@ run-call: EmptyContractRuntime::deploysRevertingRuntime() => true
// ported-from: test/libsolidity/semanticTests/constants/assign_type_info.sol

contract A {}

contract B {
    bytes constant creationCode = type(A).creationCode;
    bytes constant runtimeCode = type(A).runtimeCode;

    function nonEmptyCode() public pure returns (bool) {
        return creationCode.length > 0 && runtimeCode.length > 0;
    }
}

contract InternalContract {
    function value() internal pure returns (uint256) {
        return 1;
    }
}

contract ConstructorOnly {
    constructor() {}
}

library InternalLibrary {
    function value() internal pure returns (uint256) {
        return 1;
    }
}

contract EmptyContractRuntime {
    function hasNonCallableRuntime() external pure returns (bool) {
        return type(InternalContract).runtimeCode.length > 0
            && type(ConstructorOnly).runtimeCode.length > 0
            && type(InternalLibrary).runtimeCode.length > 0;
    }

    function deploysRevertingRuntime() external returns (bool) {
        A target = new A();
        (bool success,) = address(target).call(hex"deadbeef");
        return address(target).code.length > 0 && !success;
    }
}
