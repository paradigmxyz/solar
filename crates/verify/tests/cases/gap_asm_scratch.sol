pragma solidity ^0.8.20;
// A helper whose assembly writes only the scratch space below the free memory pointer, at
// literal offsets, leaves the memory objects passed to it alone: a loop that calls it with a
// memory argument keeps what it knew of that argument. Any other memory write in the helper's
// assembly, or an assignment there that may make a Solidity reference point at memory, still
// counts as writing the argument, which such a loop does not support.
library Scratch {
    function tag(bytes memory b, uint256 i) internal pure returns (bytes32 h) {
        uint256 len = b.length;
        assembly {
            mstore(0x00, len)
            mstore8(0x3f, i)
            h := keccak256(0x00, 0x40)
        }
    }

    function ask(address t, bytes memory b) internal returns (uint256 r) {
        uint256 len = b.length;
        assembly {
            mstore(0x00, len)
            if iszero(call(gas(), t, 0, 0x00, 0x20, 0x00, 0x20)) { revert(0, 0) }
            r := mload(0x00)
        }
    }

    function fmp(bytes memory b) internal pure {
        uint256 len = b.length;
        assembly {
            mstore(0x40, len)
        }
    }

    function computed(bytes memory b) internal pure {
        uint256 len = b.length;
        assembly {
            let p := 0x00
            mstore(p, len)
        }
    }

    function straddle(bytes memory b) internal pure {
        uint256 len = b.length;
        assembly {
            mstore(0x30, len)
        }
    }

    function copy(bytes memory b) internal pure {
        uint256 len = b.length;
        assembly {
            returndatacopy(0x00, 0x00, len)
        }
    }

    function out(address t, bytes memory b) internal {
        uint256 len = b.length;
        assembly {
            pop(call(gas(), t, 0, 0x00, 0x00, len, 0x20))
        }
    }

    // A Yul function's offsets are its parameters, not literals.
    function viaFunction(bytes memory b) internal pure {
        uint256 len = b.length;
        assembly {
            function put(p, v) {
                mstore(p, v)
            }
            put(0x00, len)
        }
    }

    function rebind(bytes memory b) internal pure {
        assembly {
            b := 0x60
        }
    }

    // `c` names `b`'s object after the assembly, so the write through `c` writes `b`.
    function aliasWrite(bytes memory b) internal pure {
        bytes memory c;
        assembly {
            c := b
        }
        c[0] = 0x01;
    }

    function literal(bytes memory b) internal pure {
        uint256 len = b.length;
        bytes memory c;
        assembly {
            c := 0x80
        }
        if (len > 0) c[0] = 0x01;
    }
}

contract AsmScratch {
    /// @custom:requires b.length == 64
    /// @custom:ensures r == 64
    function ok_loop(bytes memory b, uint256 n) public pure returns (uint256 r) {
        for (uint256 i = 0; i < n && i < 4; i++) {
            Scratch.tag(b, i);
        }
        r = b.length;
    }

    /// @custom:requires b.length == 64
    /// @custom:ensures r == 64
    function ok_call_loop(address t, bytes memory b, uint256 n) public returns (uint256 r) {
        for (uint256 i = 0; i < n && i < 4; i++) {
            Scratch.ask(t, b);
        }
        r = b.length;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == b.length
    function bad_fmp(bytes memory b, uint256 n) public pure returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            Scratch.fmp(b);
        }
        r = b.length;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == b.length
    function bad_computed(bytes memory b, uint256 n) public pure returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            Scratch.computed(b);
        }
        r = b.length;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == b.length
    function bad_straddle(bytes memory b, uint256 n) public pure returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            Scratch.straddle(b);
        }
        r = b.length;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == b.length
    function bad_copy(bytes memory b, uint256 n) public pure returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            Scratch.copy(b);
        }
        r = b.length;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == b.length
    function bad_out(address t, bytes memory b, uint256 n) public returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            Scratch.out(t, b);
        }
        r = b.length;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == b.length
    function bad_via_function(bytes memory b, uint256 n) public pure returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            Scratch.viaFunction(b);
        }
        r = b.length;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == b.length
    function bad_rebind(bytes memory b, uint256 n) public pure returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            Scratch.rebind(b);
        }
        r = b.length;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == b.length
    function bad_alias(bytes memory b, uint256 n) public pure returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            Scratch.aliasWrite(b);
        }
        r = b.length;
    }

    // expect: not supported in a loop yet
    /// @custom:ensures r == b.length
    function bad_literal(bytes memory b, uint256 n) public pure returns (uint256 r) {
        for (uint256 i = 0; i < n; i++) {
            Scratch.literal(b);
        }
        r = b.length;
    }
}
