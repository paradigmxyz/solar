//@ run-call: f() => 255, 3
//@ run-call: g() => 2

contract StorageStructBytesIndex {
    struct S {
        bytes b;
    }

    S s;

    constructor() {
        s.b = hex"010203";
    }

    function f() external returns (uint256, uint256) {
        delete s;
        s.b = hex"010203";
        s.b[0] = bytes1(0xff);
        return (uint8(s.b[0]), s.b.length);
    }

    function g() external view returns (uint256) {
        return uint8(bytes(s.b)[1]);
    }
}
