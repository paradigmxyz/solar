//@ codegen-matrix: standard
//@ compile-flags: -Zvalidate-ir=true
//@ run-call: attachedThis => true
//@ run-call: f 1
// ported-from: test/libsolidity/syntaxTests/using/library_function_attached_but_not_called.sol

library D {
    function double(uint256 self) public pure returns (uint256) {
        return 2 * self;
    }

    function selfAddress(UnusedBoundLibraryFunction receiver) internal pure returns (address) {
        return address(receiver);
    }
}

contract UnusedBoundLibraryFunction {
    using D for uint256;
    using D for UnusedBoundLibraryFunction;

    bool private constructorReceiver;

    constructor() {
        constructorReceiver = this.selfAddress() == address(this);
    }


    function f(uint256 a) external pure {
        a.double;
    }

    function attachedThis() external view returns (bool) {
        return constructorReceiver && this.selfAddress() == address(this);
    }
}
