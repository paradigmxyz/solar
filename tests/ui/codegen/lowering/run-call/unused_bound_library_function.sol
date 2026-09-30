//@ codegen-matrix: standard
//@ run-call: attachedThis => true
//@ run-call: f 1
//@ run-call: builtinReferences => 7
//@ run-call: receiverEffect => 1
//@ run-call: freeReceiverEffect => 1
//@ run-call: nestedReceiverEffect true => 1
//@ run-call-fail: nestedReceiverEffect false => 0x
//@ run-call-fail: freeCheckedReceiver => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: tupleReceiverEffect => 4
//@ run-call: tupleAssignmentEffect => 4
//@ run-call-fail: freeReceiverRevert => 0x
//@ run-call-fail: tupleReceiverRevert => 0x
//@ run-call-fail: receiverRevert => 0x
//@ run-call-fail: checkedReceiver => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
// ported-from: test/libsolidity/syntaxTests/using/library_function_attached_but_not_called.sol

library D {
    function double(uint256 self) public pure returns (uint256) {
        return 2 * self;
    }
}

function identity(uint256 self) pure returns (uint256) {
    return self;
}

contract UnusedBoundLibraryFunction {
    using D for uint256;
    using ThisMethods for UnusedBoundLibraryFunction;
    using {identity} for uint256;

    uint256 private count;
    bool private constructorReceiver;

    constructor() {
        constructorReceiver = this.selfAddress() == address(this);
    }


    function receiverEffect() external returns (uint256) {
        next().double;
        return count;
    }

    function next() internal returns (uint256) {
        return ++count;
    }

    function receiverRevert() external pure {
        fail().double;
    }

    function fail() internal pure returns (uint256) {
        revert();
    }

    function checkedReceiver() external pure {
        uint256[] memory values = new uint256[](1);
        values[1].double;
    }

    function f(uint256 a) external pure {
        a.double;
    }

    function builtinReferences() external pure returns (uint256) {
        selfdestruct;
        keccak256;
        blockhash;
        gasleft;
        return 7;
    }
    function freeReceiverEffect() external returns (uint256) {
        next().identity;
        return count;
    }

    function tupleReceiverEffect() external returns (uint256) {
        (, uint256 x) = (next().identity, 3);
        return count + x;
    }

    function tupleAssignmentEffect() external returns (uint256) {
        uint256 x;
        (, x) = (next().double, 3);
        return count + x;
    }

    function freeReceiverRevert() external pure {
        fail().identity;
    }

    function tupleReceiverRevert() external pure returns (uint256) {
        (, uint256 x) = (fail().identity, 3);
        return x;
    }
    function nestedReceiverEffect(bool first) external returns (uint256) {
        (first ? next() : fail()).identity;
        return count;
    }

    function freeCheckedReceiver() external pure {
        uint256[] memory values = new uint256[](1);
        values[1].identity;
    }
    function attachedThis() external view returns (bool) {
        return constructorReceiver && this.selfAddress() == address(this);
    }
}

library ThisMethods {
    function selfAddress(UnusedBoundLibraryFunction receiver) internal pure returns (address) {
        return address(receiver);
    }
}
