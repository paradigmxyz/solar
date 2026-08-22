//@ run-call: Harness::run() => 1

// Hand-written creation-code builders may temporarily use memory immediately
// before a heap object and restore it after `create2`. Static internal frames
// end exactly where the heap begins, so spilling the saved words into that
// prefix lets the image overwrite its own restoration state. Values live
// across a backward heap write must remain on the stack until the image is no
// longer live.

contract Implementation {
    function ping() external pure returns (uint256) {
        return 7;
    }
}

contract Harness {
    function run() external returns (uint256) {
        Implementation implementation = new Implementation();
        bytes memory data = hex"112233445566778899aabbccddeeff";
        address instance = _cloneDeterministic(address(implementation), data, bytes32(uint256(1)));
        require(Implementation(instance).ping() == 7, "proxy");
        return 1;
    }

    function _cloneDeterministic(address implementation, bytes memory data, bytes32 salt)
        internal
        returns (address instance)
    {
        assembly {
            let mBefore3 := mload(sub(data, 0x60))
            let mBefore2 := mload(sub(data, 0x40))
            let mBefore1 := mload(sub(data, 0x20))
            let dataLength := mload(data)
            let dataEnd := add(add(data, 0x20), dataLength)
            let mAfter1 := mload(dataEnd)
            let extraLength := add(dataLength, 2)

            mstore(data, 0x5af43d3d93803e606057fd5bf3)
            mstore(sub(data, 0x0d), implementation)
            mstore(
                sub(data, 0x21),
                or(shl(0x48, extraLength), 0x593da1005b363d3d373d3d3d3d610000806062363936013d73)
            )
            mstore(
                sub(data, 0x3a),
                0x9e4ac34f21c619cefc926c8bd93b54bf5a39c7ab2127a895af1cc0691d7e3dff
            )
            mstore(
                sub(data, add(0x59, lt(extraLength, 0xff9e))),
                or(shl(0x78, add(extraLength, 0x62)), 0xfd6100003d81600a3d39f336602c57343d527f)
            )
            mstore(dataEnd, shl(0xf0, extraLength))

            instance := create2(0, sub(data, 0x4c), add(extraLength, 0x6c), salt)
            if iszero(instance) { revert(0, 0) }

            mstore(dataEnd, mAfter1)
            mstore(data, dataLength)
            mstore(sub(data, 0x20), mBefore1)
            mstore(sub(data, 0x40), mBefore2)
            mstore(sub(data, 0x60), mBefore3)
        }
    }
}
