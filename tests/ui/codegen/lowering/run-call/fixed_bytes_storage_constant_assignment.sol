//@ codegen-matrix: standard
//@ run-call: structField => 0x0102
//@ run-call: stateVariable => 0x01020304

contract FixedBytesStorageConstantAssignment {
    struct S {
        bytes2 a;
        uint256 b;
    }

    bytes2 constant B2 = 0x0102;
    bytes4 constant B4 = 0x01020304;

    S s;
    bytes4 x;

    function structField() external returns (bytes2) {
        s = S(B2, 1);
        return s.a;
    }

    function stateVariable() external returns (bytes4) {
        x = B4;
        return x;
    }
}
