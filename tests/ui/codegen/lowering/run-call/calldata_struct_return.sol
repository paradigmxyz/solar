//@ codegen-matrix: standard
//@ run-call: CalldataStructReturn::roundTrip() => true, 1, 2, 3, 4, 3, 4
//@ run-call: CalldataStructReturn::direct(bytes) 0x000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000002aabb00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000046a736f6e00000000000000000000000000000000000000000000000000000000 => true, 3

library CalldataStructDecoder {
    struct Auth {
        bytes32 r;
        bytes32 s;
        uint256 challengeIndex;
        uint256 typeIndex;
        bytes authenticatorData;
        string clientDataJSON;
    }

    function decode(bytes calldata input) internal pure returns (bool success, Auth calldata auth) {
        assembly ("memory-safe") {
            auth := input.offset
        }
        return (true, auth);
    }
}

contract CalldataStructReturn {
    function direct(bytes calldata encoded) external pure returns (bool success, uint256 value) {
        CalldataStructDecoder.Auth calldata auth;
        (success, auth) = CalldataStructDecoder.decode(encoded);
        value = auth.challengeIndex;
    }

    function decode(bytes calldata encoded)
        external
        pure
        returns (bool success, CalldataStructDecoder.Auth calldata auth)
    {
        (success, auth) = CalldataStructDecoder.decode(encoded);
    }

    function roundTrip()
        external
        view
        returns (bool, uint256, uint256, uint256, uint256, uint256, uint256)
    {
        (bool success, CalldataStructDecoder.Auth memory auth) = this.decode(
            abi.encode(bytes32(uint256(1)), bytes32(uint256(2)), uint256(3), uint256(4), hex"aabbcc", "json")
        );
        return (
            success,
            uint256(auth.r),
            uint256(auth.s),
            auth.challengeIndex,
            auth.typeIndex,
            auth.authenticatorData.length,
            bytes(auth.clientDataJSON).length
        );
    }
}
