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
    // CHECK: [[A0:v[0-9]+]] = assign 77
    // CHECK: [[A1:v[0-9]+]] = assign 0x78797a
    // CHECK: [[A2:v[0-9]+]] = assign 42
    // CHECK: [[A3:v[0-9]+]] = assign 0x616263
    // CHECK: [[R4:v[0-9]+]] = insert_value struct0, undef struct0, 0, [[A3]]
    // CHECK: [[R5:v[0-9]+]] = insert_value struct0, [[R4]], 1, [[A2]]
    // CHECK: [[R6:v[0-9]+]] = insert_value struct0, [[R5]], 2, [[A1]]
    // CHECK: [[R7:v[0-9]+]] = insert_value struct0, [[R6]], 3, [[A0]]
    function twoComponents() external pure returns (bytes32 b, uint256 x, bytes32 c, uint256 y) {
        ((b, x), (c, y)) = (("abc", 0x2a), ("xyz", 0x4d));
    }

    // CHECK-LABEL: fn @tripleNested()
    // CHECK: [[A0:v[0-9]+]] = assign 99
    // CHECK: [[A1:v[0-9]+]] = assign 77
    // CHECK: [[A2:v[0-9]+]] = assign 42
    // CHECK: [[A3:v[0-9]+]] = assign 0x616263
    // CHECK: [[R4:v[0-9]+]] = insert_value struct0, undef struct0, 0, [[A3]]
    // CHECK: [[R5:v[0-9]+]] = insert_value struct0, [[R4]], 1, [[A2]]
    // CHECK: [[R6:v[0-9]+]] = insert_value struct0, [[R5]], 2, [[A1]]
    // CHECK: [[R7:v[0-9]+]] = insert_value struct0, [[R6]], 3, [[A0]]
    function tripleNested() external pure returns (bytes32 b, uint256 x, uint256 y, uint256 z) {
        (((b, x), y), z) = ((("abc", 0x2a), 0x4d), 0x63);
    }

    // CHECK-LABEL: fn @rotateUp()
    // CHECK: [[A0:v[0-9]+]] = assign 3
    // CHECK: [[A1:v[0-9]+]] = assign 2
    // CHECK: [[A2:v[0-9]+]] = assign 1
    // CHECK: [[A3:v[0-9]+]] = assign 0x78797a
    // CHECK: [[A4:v[0-9]+]] = assign 0x616263
    // CHECK: [[R5:v[0-9]+]] = insert_value struct1, undef struct1, 0, [[A4]]
    // CHECK: [[R6:v[0-9]+]] = insert_value struct1, [[R5]], 1, [[A3]]
    // CHECK: [[R7:v[0-9]+]] = insert_value struct1, [[R6]], 2, [[A2]]
    // CHECK: [[R8:v[0-9]+]] = insert_value struct1, [[R7]], 3, [[A1]]
    // CHECK: [[R9:v[0-9]+]] = insert_value struct1, [[R8]], 4, [[A0]]
    function rotateUp() external pure returns (bytes32 b, bytes32 c, uint256 x, uint256 y, uint256 z) {
        ((b, c, x), y, z) = (("abc", "xyz", 1), 2, 3);
    }

    // CHECK-LABEL: fn @middleComponent()
    // CHECK: [[A0:v[0-9]+]] = assign 99
    // CHECK: [[A1:v[0-9]+]] = assign 42
    // CHECK: [[A2:v[0-9]+]] = assign 0x616263
    // CHECK: [[A3:v[0-9]+]] = assign 77
    // CHECK: [[R4:v[0-9]+]] = insert_value struct0, undef struct0, 0, [[A3]]
    // CHECK: [[R5:v[0-9]+]] = insert_value struct0, [[R4]], 1, [[A2]]
    // CHECK: [[R6:v[0-9]+]] = insert_value struct0, [[R5]], 2, [[A1]]
    // CHECK: [[R7:v[0-9]+]] = insert_value struct0, [[R6]], 3, [[A0]]
    function middleComponent() external pure returns (uint256 x, bytes32 b, uint256 y, uint256 z) {
        (x, (b, y), z) = (0x4d, ("abc", 0x2a), 0x63);
    }

    // CHECK-LABEL: fn @lastComponent()
    // CHECK: [[A0:v[0-9]+]] = assign 42
    // CHECK: [[A1:v[0-9]+]] = assign 0x616263
    // CHECK: [[A2:v[0-9]+]] = assign 77
    // CHECK: [[R3:v[0-9]+]] = insert_value struct2, undef struct2, 0, [[A2]]
    // CHECK: [[R4:v[0-9]+]] = insert_value struct2, [[R3]], 1, [[A1]]
    // CHECK: [[R5:v[0-9]+]] = insert_value struct2, [[R4]], 2, [[A0]]
    function lastComponent() external pure returns (uint256 x, bytes32 b, uint256 y) {
        (x, (b, y)) = (0x4d, ("abc", 0x2a));
    }
}
