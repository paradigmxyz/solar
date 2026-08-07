//@ run-call: f() => 0xba4f20407251e4607cd66b90bfea19ec6971699c03e4a4f3ea737d5818ac27ae, 0xba4f20407251e4607cd66b90bfea19ec6971699c03e4a4f3ea737d5818ac27ae, 0xe7490fade3a8e31113ecb6c0d2635e28a6f5ca8359a57afe914827f41ddf0848
//@ run-call: hashAbi() => 0x6758b4e9d54934cf885d10dfefcaa5d32df1735663c7bc76ede308ee24c8176d
//@ run-call: hashNestedAbi() => 0x73e4b829b5f2f329d3f30cdff339d9767485f927c5af72e41bc3870e933743a4
//@ run-call: hashStructAbi() => 0xba4f20407251e4607cd66b90bfea19ec6971699c03e4a4f3ea737d5818ac27ae
//@ run-call: hashScalarAbi() => 0x405787fa12a823e0f2b7631cc41b3ba8828b3321ca811111fa75cd3aa3bb5ace
//@ run-call: scalarCall() => 2
// ported-from: test/libsolidity/semanticTests/builtinFunctions/keccak256_packed_complex_types.sol

contract AbiPackedComplexTypes {
    uint120[3] x;

    struct S {
        uint120 a;
        uint120[2] b;
    }

    function f() external returns (bytes32 hash1, bytes32 hash2, bytes32 hash3) {
        uint120[] memory y = new uint120[](3);
        x[0] = y[0] = uint120(type(uint).max - 1);
        x[1] = y[1] = uint120(type(uint).max - 2);
        x[2] = y[2] = uint120(type(uint).max - 3);
        hash1 = keccak256(abi.encodePacked(x));
        hash2 = keccak256(abi.encodePacked(y));
        hash3 = keccak256(abi.encodePacked(AbiPackedComplexTypes(address(0x1234))));
    }

    function hashAbi() external pure returns (bytes32) {
        uint120[] memory y = new uint120[](3);
        y[0] = uint120(type(uint).max - 1);
        y[1] = uint120(type(uint).max - 2);
        y[2] = uint120(type(uint).max - 3);
        return keccak256(abi.encode(y));
    }

    function hashNestedAbi() external pure returns (bytes32) {
        uint120[2][] memory y = new uint120[2][](1);
        y[0][0] = uint120(type(uint).max - 1);
        y[0][1] = uint120(type(uint).max - 2);
        return keccak256(abi.encode(y));
    }

    function hashStructAbi() external pure returns (bytes32) {
        S memory s;
        s.a = uint120(type(uint).max - 1);
        s.b[0] = uint120(type(uint).max - 2);
        s.b[1] = uint120(type(uint).max - 3);
        return keccak256(abi.encode(s));
    }

    function hashScalarAbi() external pure returns (bytes32) {
        uint8 x;
        assembly {
            x := 0x0102
        }
        return keccak256(abi.encode(x));
    }

    function scalarCall() external view returns (uint256) {
        uint8 x;
        assembly {
            x := 0x0102
        }
        return this.echoScalar(x);
    }

    function echoScalar(uint8 x) external pure returns (uint256) {
        return x;
    }
}
