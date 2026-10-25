//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Osize -Zdump=mir
//@[dump] filecheck:

interface Vm {
    function toStringUint(uint256 value) external pure returns (string memory);
    function toStringBool(bool value) external pure returns (string memory);
}

// `paintUint` and `paintBool` each call one cheatcode and wrap the result through the same
// helper, and their code differs only in a selector and an argument. The backend shares
// everything after those while both stay functions, so consuming either into its single caller
// would emit the shared run again.
contract SharedRuns {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    event Painted(string);

    // CHECK-LABEL: fn @paint()
    // CHECK: icall @paintUint
    // CHECK: icall @paintBool
    function paint() external {
        emit Painted(paintUint(10e18));
        emit Painted(paintBool(true));
    }

    function paintUint(uint256 value) private pure returns (string memory) {
        return wrap(vm.toStringUint(value));
    }

    function paintBool(bool value) private pure returns (string memory) {
        return wrap(vm.toStringBool(value));
    }

    function wrap(string memory text) private pure returns (string memory) {
        return string(abi.encodePacked("<", text, ">"));
    }
}
