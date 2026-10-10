//@ compile-flags: -O gas -Zdump=evm-ir-runtime
//@ filecheck:
//@ normalize-stdout-test: "(?s).+" -> ""

// Both arrays stay on the stack across the low-memory copy, so staging the
// child's creation code reaches its size and destination with shallow `dup`s.
// CHECK: push_data Child_creation_code_0
// CHECK-NEXT: dup 3
// CHECK-NEXT: codecopy
// CHECK-NEXT: push {{[0-9]+}}
// CHECK-NEXT: dup 2
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
