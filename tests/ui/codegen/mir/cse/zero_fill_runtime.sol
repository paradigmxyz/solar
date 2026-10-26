//@ codegen-matrix: standard
//@ run-call: get 0x0000000000000000000000000000000000ab1234, 7 => (0, 7, 0, 0, 0x1234, 0x0000000000000000000000000000000000000000, 0)
//@ run-call: get 0x0000000000000000000000000000000000000000, 0 => (0, 0, 0, 0, 0, 0x0000000000000000000000000000000000000000, 0)
//@ run-call: overwrite 9, 0 => (9, 0, 0)
//@ run-call: overwrite 9, 1 => (0, 9, 0)
//@ run-call: overwrite 9, 2 => (0, 0, 9)

contract ZeroFill {
    struct S {
        uint128 a;
        uint128 b;
        uint128 c;
        uint40 d;
        uint16 id;
        address x;
        uint8 e;
    }

    struct T {
        uint256 a;
        uint256 b;
        uint256 c;
    }

    function get(address asset, uint128 b) external pure returns (S memory data) {
        data.id = uint16(uint160(asset));
        data.b = b;
    }

    function overwrite(uint256 value, uint256 index) external pure returns (T memory t) {
        assembly {
            mstore(add(t, mul(mod(index, 3), 32)), value)
        }
    }
}
