//@ codegen-matrix: standard
//@ run-call: output => 0x1234000000000000000000000000000000000000000000000000000000000000

contract MultiReturnUnbumpedMemory {
    function output() external pure returns (bytes32 result) {
        assembly {
            function emitWord(cursor) -> next, ignored {
                mstore(cursor, shl(240, 0x1234))
                next := add(cursor, 2)
            }

            let start := add(mload(0x40), 0x20)
            let cursor := start
            let ignored
            cursor, ignored := emitWord(cursor)
            result := mload(start)
        }
    }
}
