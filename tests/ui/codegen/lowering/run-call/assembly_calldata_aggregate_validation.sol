//@ codegen-matrix: standard
//@ run-call-fail: 0x1bc4b3c7ffff000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call: 0x1bc4b3c70000000000000000000000000000000000000000000000000000000000000005 => 0x968ff3173f5603f38d885e85543bfdbc7992e4d66d6221e7e7e1a4b4f8b78ba1
//@ run-call-fail: 0x478b5525ffff000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call: 0x478b55250000000000000000000000000000000000000000000000000000000000000005 => 0x968ff3173f5603f38d885e85543bfdbc7992e4d66d6221e7e7e1a4b4f8b78ba1
//@ run-call: 0x5b524347000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020102000000000000000000000000000000000000000000000000000000000000 => 0xd23fc6df38a6e073d0ff3995a6b7606a7d287099869ec138a386769b04982387
//@ run-call-fail: 0xcf7d0a4cffff000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call: 0xcf7d0a4c0000000000000000000000000000000000000000000000000000000000000005 => 0x2b232c97452f0950c94e2539fdc7e69d21166113cf7a9bcb99b220a3fe5d720a
//@ run-call-fail: 0xa11cea7bffff000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call: 0xa11cea7b0000000000000000000000000000000000000000000000000000000000000005 => 0x2b232c97452f0950c94e2539fdc7e69d21166113cf7a9bcb99b220a3fe5d720a
//@ run-call-fail: 0xd64476830000000000000000000000000000000000000000000000000000000000000002 => 0x
//@ run-call: 0xd64476830000000000000000000000000000000000000000000000000000000000000001 => 0x1e7c5c1c118b439a090ebf565465179476e94bae5ba6a5ae0f146ec3866c8795
//@ run-call-fail: 0x977eca9cffff000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call: 0x977eca9c0000000000000000000000000000000000000000000000000000000000000005 => 0x036b6384b5eca791c62761152d0c79bb0604c104a5fb6f4eb0703f3154bb3db0
//@ run-call-fail: 0xf3f1126affff000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call: 0xf3f1126a0000000000000000000000000000000000000000000000000000000000000005 => 0x0000000000000000000000000000000000000000000000000000000000000001
//@ run-call-fail: 0x8f1fefadffff000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call: 0x8f1fefad0000000000000000000000000000000000000000000000000000000000000005 => 0x0000000000000000000000000000000000000000000000000000000000000002
//@ run-call-fail: 0x1d045b60ffff000000000000000000000000000000000000000000000000000000000001 => 0x
//@ run-call: 0x1d045b600000000000000000000000000000000000000000000000000000000000000005 => 0x0000000000000000000000000000000000000000000000000000000000000003

// The words of a calldata array or struct whose range assembly set are
// validated as they are read, like any others: encoding, packing or logging
// one reverts when a word is dirty, whether the variable is encoded where
// assembly set it or passed to an internal function first. Its range is not
// checked: words past the end of the calldata read as zeros.
// Every result is solc 0.8.37's with --via-ir.
contract AssemblyCalldataAggregateValidation {
    struct S {
        uint8 a;
        address b;
    }

    enum E {
        A,
        B
    }

    event Words(address[] x);
    event Indexed(address[] indexed x);

    function encodeArray(address[] calldata x) internal pure returns (bytes32) {
        return keccak256(abi.encode(x));
    }

    function encodeStruct(S calldata s) internal pure returns (bytes32) {
        return keccak256(abi.encode(s));
    }

    function logArray(address[] calldata x) internal {
        emit Words(x);
    }

    function arrayDirect(uint256) external pure returns (bytes32) {
        address[] calldata x;
        assembly {
            x.offset := 4
            x.length := 1
        }
        return keccak256(abi.encode(x));
    }

    function arrayInternal(uint256) external pure returns (bytes32) {
        address[] calldata x;
        assembly {
            x.offset := 4
            x.length := 1
        }
        return encodeArray(x);
    }

    function arrayPastEnd(bytes calldata data) external pure returns (bytes32) {
        address[] calldata x;
        assembly {
            x.offset := add(data.offset, data.length)
            x.length := 3
        }
        return encodeArray(x);
    }

    function structDirect(uint256) external pure returns (bytes32) {
        S calldata s;
        assembly {
            s := 4
        }
        return keccak256(abi.encode(s));
    }

    function structInternal(uint256) external pure returns (bytes32) {
        S calldata s;
        assembly {
            s := 4
        }
        return encodeStruct(s);
    }

    function enumArray(uint256) external pure returns (bytes32) {
        E[] calldata x;
        assembly {
            x.offset := 4
            x.length := 1
        }
        return keccak256(abi.encode(x));
    }

    function packed(uint256) external pure returns (bytes32) {
        address[] calldata x;
        assembly {
            x.offset := 4
            x.length := 1
        }
        return keccak256(abi.encodePacked(x));
    }

    function logged(uint256) external returns (uint256) {
        address[] calldata x;
        assembly {
            x.offset := 4
            x.length := 1
        }
        emit Words(x);
        return 1;
    }

    function loggedInternal(uint256) external returns (uint256) {
        address[] calldata x;
        assembly {
            x.offset := 4
            x.length := 1
        }
        logArray(x);
        return 2;
    }

    function loggedIndexed(uint256) external returns (uint256) {
        address[] calldata x;
        assembly {
            x.offset := 4
            x.length := 1
        }
        emit Indexed(x);
        return 3;
    }
}
