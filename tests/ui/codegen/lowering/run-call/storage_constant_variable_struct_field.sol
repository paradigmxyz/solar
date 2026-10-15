//@ codegen-matrix: standard
//@ run-call: f => 0x7521d1cadbcfa91eec65aa16715b94ffc1c9654ba57ea2ef1a2127bca1127a83, 0x0000000000000000000000000000000000000001

contract StorageConstantVariableStructField {
    struct S {
        uint256 a;
        address b;
    }

    uint256 constant H = uint256(keccak256("x"));
    address constant A = address(1);

    S s;

    function f() external returns (uint256, address) {
        s = S(H, A);
        return (s.a, s.b);
    }
}
