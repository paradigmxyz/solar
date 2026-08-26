//@ codegen-matrix: standard
//@ run-call: test() => false
//@ run-call: testFlags() => true
//@ run-call: testTypedConstant() => true

contract C {
    bytes1 private constant AUTH_DATA_FLAGS_UP = 0x01;

    struct Auth {
        bytes authenticatorData;
        string clientDataJSON;
        uint256 challengeIndex;
        uint256 typeIndex;
        bytes32 r;
        bytes32 s;
    }

    function test() external pure returns (bool) {
        return check(true, auth());
    }

    function testFlags() external pure returns (bool) {
        Auth memory value = auth();
        bool result;
        assembly {
            let u := 1
            result := eq(and(mload(add(mload(value), 0x21)), u), u)
        }
        return result;
    }

    function testTypedConstant() external pure returns (bool) {
        bytes memory authenticatorData = auth().authenticatorData;
        return authenticatorData[32] & AUTH_DATA_FLAGS_UP == AUTH_DATA_FLAGS_UP;
    }

    function auth() internal pure returns (Auth memory auth) {
        auth.authenticatorData =
            hex"68de9469627f4002c727fe634985a916f4414d6309d81cc2a26ca3f1785725b649ddbb03b8b89d41b77e5e5ad00b388d3e95043177ee2ef763a67a3e2cd4";
        auth.clientDataJSON = string(
            abi.encodePacked(
                hex"7b252274797065223a22776562617574686e2e676574220ff9062c226368616c6c656e6765223a22466722d2df7d"
            )
        );
        auth.challengeIndex = 27;
        auth.typeIndex = 2;
    }

    function check(bool requireUserVerification, Auth memory auth)
        internal
        pure
        returns (bool result)
    {
        bytes memory encoded = hex"4667";
        assembly {
            let clientDataJSON := mload(add(auth, 0x20))
            let n := mload(clientDataJSON)
            let o := add(clientDataJSON, 0x20)
            {
                let c := mload(add(auth, 0x40))
                let t := mload(add(auth, 0x60))
                let l := mload(encoded)
                let q := add(l, 0x0d)
                mstore(encoded, shr(152, '"challenge":"'))
                result := and(
                    and(
                        eq(shr(88, mload(add(o, t))), shr(88, '"type":"webauthn.get"')),
                        lt(shr(128, or(t, c)), lt(add(0x14, t), n))
                    ),
                    and(
                        eq(keccak256(add(o, c), q), keccak256(add(encoded, 0x13), q)),
                        and(eq(byte(0, mload(add(add(o, c), q))), 34), lt(add(q, c), n))
                    )
                )
                mstore(encoded, l)
            }
            let l := mload(mload(auth))
            let u := or(1, shl(2, iszero(iszero(requireUserVerification))))
            result := and(and(result, gt(l, 0x20)), eq(and(mload(add(mload(auth), 0x21)), u), u))
        }
    }
}
