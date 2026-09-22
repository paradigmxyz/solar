//@ codegen-matrix: standard
//@ run-call: f 1
//@ run-call: builtinReferences => 7
//@ run-call: receiverEffect => 1
//@ run-call-fail: receiverRevert => 0x
//@ run-call-fail: checkedReceiver => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
// ported-from: test/libsolidity/syntaxTests/using/library_function_attached_but_not_called.sol

library D {
    function double(uint256 self) public pure returns (uint256) {
        return 2 * self;
    }
}

contract UnusedBoundLibraryFunction {
    using D for uint256;

    uint256 private count;

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
}
