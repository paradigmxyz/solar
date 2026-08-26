//@ codegen-matrix: standard
//@ run-call: C::builtinOrder() => 50
//@ run-call: C::userFnOrder() => 100
//@ run-call: C::deployIdiom() => 1

// Yul evaluates call arguments right to left. solady's CREATE3-style deploy
// idiom `mul(extcodesize(target), call(...))` depends on the call executing
// before the code-size check; left-to-right evaluation read size zero and
// reverted every deterministic deployment.

contract C {
    function builtinOrder() external pure returns (uint256 r) {
        assembly {
            function eff() -> x {
                mstore(0x00, 5)
                x := 10
            }
            mstore(0x00, 3)
            r := mul(mload(0x00), eff())
        }
    }

    function userFnOrder() external pure returns (uint256 r) {
        assembly {
            function w(v) -> x {
                mstore(0x00, v)
                x := v
            }
            mstore(0x00, 1)
            r := sub(add(mload(0x00), 100), w(7))
        }
    }

    function deployIdiom() external returns (uint256 r) {
        // Initcode returning a 10-byte runtime (PUSH1 7 MSTORE RETURN).
        bytes memory initCode = hex"600a600a5f39600a5ff3600760005260206000f3";
        assembly {
            function doCreate(ic) -> addr {
                addr := create(0, add(ic, 0x20), mload(ic))
                mstore(0x20, addr)
            }
            mstore(0x20, 0)
            // Right to left: the create runs first and records its address;
            // the code-size check then observes the fresh deployment.
            r := iszero(iszero(mul(extcodesize(mload(0x20)), doCreate(initCode))))
        }
    }
}
