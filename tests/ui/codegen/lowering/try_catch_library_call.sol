//@ codegen-matrix: standard unlinked
//@[none, gas, size, mir] compile-flags: --libraries Lib=0x1111111111111111111111111111111111111111
//@[mir] filecheck: --check-prefix=LINKED
//@[unlinked] compile-flags: -O none
//@[none, gas, size, mir] run-call-fail: C::emptyCode() => 0x
// ported-from: test/libsolidity/semanticTests/tryCatch/try_catch_library_call.sol

library Lib {
    struct S {
        uint256 value;
    }

    function direct(uint256 value, bool fail) external pure returns (uint256) {
        require(!fail, "failed");
        return value;
    }

    function attached(S storage self, uint256 value, bool fail) public returns (uint256) {
        require(!fail, "failed");
        self.value = value;
        return self.value;
    }

    function add(uint256 self, uint256 value, bool fail) public pure returns (uint256) {
        require(!fail, "failed");
        return self + value;
    }

    function empty() public pure {}
}

contract C {
    using Lib for Lib.S;
    using Lib for uint256;

    Lib.S private state;

    // LINKED-LABEL: @module C
    // LINKED-LABEL: fn @direct
    // LINKED: abi_encode [word, word], selector 0x52db6885{{.*}}, args 8,
    // LINKED: delegatecall {{.*}}, 0x1111111111111111111111111111111111111111,
    function direct(bool fail) external pure returns (uint256) {
        try Lib.direct({fail: fail, value: 8}) returns (uint256 value) {
            return value;
        } catch Error(string memory) {
            return 18;
        }
    }

    // LINKED-LABEL: fn @attached
    // LINKED: abi_encode [word, word, word], selector 0x280ac7e9{{.*}}, args 0, 9,
    // LINKED: delegatecall {{.*}}, 0x1111111111111111111111111111111111111111,
    function attached(bool fail) external returns (uint256) {
        try state.attached({fail: fail, value: 9}) returns (uint256 value) {
            return value;
        } catch Error(string memory) {
            return 19;
        }
    }

    // LINKED-LABEL: fn @attachedValue
    // LINKED: abi_encode [word, word, word], selector 0x7f6a6c2{{.*}}, args arg0, 10,
    // LINKED: delegatecall {{.*}}, 0x1111111111111111111111111111111111111111,
    function attachedValue(uint256 self, bool fail) external pure returns (uint256) {
        try self.add({fail: fail, value: 10}) returns (uint256 value) {
            return value;
        } catch Error(string memory) {
            return 20;
        }
    }

    // LINKED-LABEL: fn @emptyCode
    // LINKED: abi_encode [], selector 0xf2a75fe4{{.*}}
    // LINKED: extcodesize 0x1111111111111111111111111111111111111111
    // LINKED: delegatecall {{.*}}, 0x1111111111111111111111111111111111111111,
    function emptyCode() external pure {
        try Lib.empty() {} catch {
            revert("caught");
        }
    }
}
