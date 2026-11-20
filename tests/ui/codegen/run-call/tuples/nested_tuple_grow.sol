//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: twoComponents => 0x6162630000000000000000000000000000000000000000000000000000000000, 0x2a, 0x78797a0000000000000000000000000000000000000000000000000000000000, 0x4d
//@ run-call: tripleNested => 0x6162630000000000000000000000000000000000000000000000000000000000, 0x2a, 0x4d, 0x63
//@ run-call: rotateUp => 0x6162630000000000000000000000000000000000000000000000000000000000, 0x78797a0000000000000000000000000000000000000000000000000000000000, 1, 2, 3
//@ run-call: middleComponent => 0x4d, 0x6162630000000000000000000000000000000000000000000000000000000000, 0x2a, 0x63
//@ run-call: lastComponent => 0x4d, 0x6162630000000000000000000000000000000000000000000000000000000000, 0x2a
// ported-from: test/libsolidity/semanticTests/types/tuple_assign_multi_slot_grow_two_components.sol
// ported-from: test/libsolidity/semanticTests/types/tuple_assign_multi_slot_grow_triple_nested.sol
// ported-from: test/libsolidity/semanticTests/types/tuple_assign_multi_slot_grow_rotate_up.sol
// ported-from: test/libsolidity/semanticTests/types/tuple_assign_multi_slot_grow_middle_component.sol
// ported-from: test/libsolidity/semanticTests/types/tuple_assign_multi_slot_grow_last_component.sol

// A nested component of a tuple assignment that occupies a stack slot and grows when converted, such
// as a string literal that becomes a `bytes32`, must not shift the other components.
contract NestedTupleGrow {
    // CHECK-LABEL: fn @twoComponents()
    // CHECK: insert_value struct0, undef struct0, 0, 0x616263
    // CHECK: insert_value struct0, v0, 1, 42
    // CHECK: insert_value struct0, v1, 2, 0x78797a
    // CHECK: insert_value struct0, v2, 3, 77
    function twoComponents() external pure returns (bytes32 b, uint256 x, bytes32 c, uint256 y) {
        ((b, x), (c, y)) = (("abc", 0x2a), ("xyz", 0x4d));
    }

    // CHECK-LABEL: fn @tripleNested()
    // CHECK: insert_value struct0, undef struct0, 0, 0x616263
    // CHECK: insert_value struct0, v0, 1, 42
    // CHECK: insert_value struct0, v1, 2, 77
    // CHECK: insert_value struct0, v2, 3, 99
    function tripleNested() external pure returns (bytes32 b, uint256 x, uint256 y, uint256 z) {
        (((b, x), y), z) = ((("abc", 0x2a), 0x4d), 0x63);
    }

    // CHECK-LABEL: fn @rotateUp()
    // CHECK: insert_value struct1, undef struct1, 0, 0x616263
    // CHECK: insert_value struct1, v0, 1, 0x78797a
    // CHECK: insert_value struct1, v1, 2, 1
    // CHECK: insert_value struct1, v2, 3, 2
    // CHECK: insert_value struct1, v3, 4, 3
    function rotateUp() external pure returns (bytes32 b, bytes32 c, uint256 x, uint256 y, uint256 z) {
        ((b, c, x), y, z) = (("abc", "xyz", 1), 2, 3);
    }

    // CHECK-LABEL: fn @middleComponent()
    // CHECK: insert_value struct0, undef struct0, 0, 77
    // CHECK: insert_value struct0, v0, 1, 0x616263
    // CHECK: insert_value struct0, v1, 2, 42
    // CHECK: insert_value struct0, v2, 3, 99
    function middleComponent() external pure returns (uint256 x, bytes32 b, uint256 y, uint256 z) {
        (x, (b, y), z) = (0x4d, ("abc", 0x2a), 0x63);
    }

    // CHECK-LABEL: fn @lastComponent()
    // CHECK: insert_value struct2, undef struct2, 0, 77
    // CHECK: insert_value struct2, v0, 1, 0x616263
    // CHECK: insert_value struct2, v1, 2, 42
    function lastComponent() external pure returns (uint256 x, bytes32 b, uint256 y) {
        (x, (b, y)) = (0x4d, ("abc", 0x2a));
    }
}
