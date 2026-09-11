//@ compile-flags: --evm-version paris
//@ codegen-matrix: standard
//@ run-call: digest; constructor=[0x010203, 0x04050607] => 0xcb9ed46f123f7ff614c914e851a7eb785ba8b8db6e5c80de71a33654fd39be9a
//@ run-call: runtimeDigest(bytes,bytes) 0x010203, 0x04050607; constructor=[0x010203, 0x04050607] => 0xcb9ed46f123f7ff614c914e851a7eb785ba8b8db6e5c80de71a33654fd39be9a

contract ConstructorMCopyHelper {
    bytes32 public immutable digest;

    constructor(bytes memory first, bytes memory second) {
        digest = keccak256(abi.encode(first, second));
    }

    function runtimeDigest(bytes memory first, bytes memory second)
        external
        pure
        returns (bytes32)
    {
        return keccak256(abi.encode(first, second));
    }
}
