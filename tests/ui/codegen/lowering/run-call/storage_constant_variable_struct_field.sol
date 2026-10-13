//@ codegen-matrix: standard
//@ run-call: f => 0x7521d1cadbcfa91eec65aa16715b94ffc1c9654ba57ea2ef1a2127bca1127a83, 0x0000000000000000000000000000000000000001
//@ run-call-fail: overflowBeforeIndex 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: namedSourceOrder => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract StorageConstantVariableStructField {
    struct S {
        uint256 a;
        address b;
    }

    enum E {
        X
    }

    struct T {
        uint256 a;
        uint256 b;
    }

    uint256 constant H = uint256(keccak256("x"));
    address constant A = address(1);
    uint256 constant MAX = type(uint256).max;
    uint256 constant OVERFLOW = MAX + 1;
    uint256 constant ZERO = 0;
    uint256 constant BAD_ENUM = uint256(E(ZERO + 5));

    S s;
    S[] arr;
    T t;

    function f() external returns (uint256, address) {
        s = S(H, A);
        return (s.a, s.b);
    }

    function overflowBeforeIndex(uint256 i) external {
        arr[i] = S(OVERFLOW, A);
    }

    function namedSourceOrder() external {
        t = T({b: OVERFLOW, a: BAD_ENUM});
    }
}
