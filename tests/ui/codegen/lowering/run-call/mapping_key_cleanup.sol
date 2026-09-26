//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: test_cleanup => true
//@ run-call: signedKey => 7
//@ run-call: signedUserKey => 9
// ported-from: test/libsolidity/semanticTests/viaYul/storage/mappings.sol

type SignedKey is int16;

contract C {
    mapping(uint16 => uint) cleanup;
    mapping(int8 => uint) signedValues;
    mapping(SignedKey => uint) signedUserValues;

    function signedUserKey() external returns (uint value) {
        signedUserValues[SignedKey.wrap(-1)] = 9;
        assembly {
            mstore(0, not(0))
            mstore(32, signedUserValues.slot)
            value := sload(keccak256(0, 64))
        }
    }

    function signedKey() external returns (uint value) {
        signedValues[-1] = 7;
        assembly {
            mstore(0, not(0))
            mstore(32, signedValues.slot)
            value := sload(keccak256(0, 64))
        }
    }

    function test_cleanup() public returns (bool) {
        uint16 x;
        assembly {
            x := 0xffff0001
        }
        cleanup[x] = 3;
        return cleanup[1] == 3;
    }
}
