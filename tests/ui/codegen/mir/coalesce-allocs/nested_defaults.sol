//@ codegen-matrix: standard
//@ run-call: fingerprint 7 => 0xf625fba444540c4f93055026dfef3a5db5ff2eae84b2603405c0ce0c2369f7b4
//@ run-call: fingerprint 0 => 0x0462e39703ac2efb1505c233e6f816f7a686a1922cda98be44cab1a3acd6b8e3

contract NestedDefaults {
    struct Array {
        uint256[] inner;
    }

    struct Stack {
        Array proved;
        uint256 remaining;
    }

    // A default Stack initializes its nested Array before its first word. The prior populated
    // Stack bounds the allocation group, so the following allocation can share the same bump.
    function fingerprint(uint256 x) public pure returns (bytes32) {
        Array memory array = Array(new uint256[](0));
        Stack memory populated = Stack(array, x);
        Stack memory empty;
        Stack memory last = Stack(array, x);
        return keccak256(abi.encode(populated, empty, last));
    }
}
