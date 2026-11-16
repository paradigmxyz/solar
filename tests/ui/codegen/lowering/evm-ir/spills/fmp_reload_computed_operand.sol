//@ compile-flags: -O gas --evm-version paris
//@ run-call-fail: run => Panic(0x11)

interface Vm {
    function expectRevert(bytes calldata) external;
}

library stdError {
    bytes public constant arithmeticError = abi.encodeWithSignature("Panic(uint256)", 0x11);
}

// Dynamic revert data followed by a self-call exercises the stale FMP reload path.
contract FmpReloadComputedOperand {
    Vm private constant vm = Vm(address(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D));

    function callWithFmp(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }

    function run() external {
        (bool success,) = address(vm).call(
            abi.encodeCall(Vm.expectRevert, (stdError.arithmeticError))
        );
        require(success);
        this.callWithFmp(2 ** 256 - 1, 2 ** 256 - 1);
    }
}
