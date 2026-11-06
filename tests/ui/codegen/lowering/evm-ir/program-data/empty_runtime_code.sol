//@ codegen-matrix: standard
//@ run-call: EmptyFactory::runtimeLength => 0
//@ run-call: EmptyFactory::runtimeCode => 0x
//@ run-call: EmptyFactory::deploy => true

contract EmptyRuntime {
    fallback() external payable {}
}

contract EmptyFactory {
    function runtimeLength() external pure returns (uint256) {
        return type(EmptyRuntime).runtimeCode.length;
    }

    function runtimeCode() external pure returns (bytes memory) {
        return type(EmptyRuntime).runtimeCode;
    }

    function deploy() external returns (bool) {
        return address(new EmptyRuntime()).code.length == 0;
    }
}
