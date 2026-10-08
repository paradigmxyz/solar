//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Keeping both arrays on the stack across the low-memory copy leaves a
// redundant copy of a word on top. Gas mode pops it while arranging the next
// operands, so staging the child's creation code needs no deeper `dup`s.
// CHECK: push_data Child_creation_code_0
// CHECK-NEXT: dup 3
// CHECK-NEXT: codecopy
// CHECK-NEXT: swap 2
// CHECK-NEXT: dup 3
// CHECK-NEXT: add
contract Child {
    constructor(address[] memory, uint256[] memory, uint256) {}
}

contract RedundantOperandPop {
    function createPair() external returns (address) {
        address[] memory receivers = new address[](2);
        uint256[] memory batches = new uint256[](2);
        assembly {
            returndatacopy(0, 0, returndatasize())
        }
        return address(new Child(receivers, batches, 17));
    }

    function createSingle() external returns (address) {
        uint256[] memory batches = new uint256[](2);
        assembly {
            returndatacopy(0, 0, returndatasize())
        }
        return address(new Child(new address[](1), batches, 5));
    }
}
