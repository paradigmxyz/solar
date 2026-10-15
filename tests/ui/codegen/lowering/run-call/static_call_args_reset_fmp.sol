//@ codegen-matrix: standard
//@ run-call: run 0 => 75
//@ run-call: run 3 => 2136
//@ run-call: runInternal 0 => 75
//@ run-call: runInternal 3 => 2136

// After a free-memory-pointer reset, every value live across the internal call must stay on
// the stack. The last stack-passed argument dies at the call, so it was preserved below the
// seven arguments that outlive it; the return label and those arguments then buried it beyond
// `DUP16`. `run` makes the call from the entry function, `runInternal` from an internal one.
contract StaticCallArgsResetFmp {
    function run(uint256 x) external pure returns (uint256) {
        unchecked {
            uint256 a = mix(x, 1);
            uint256 b = mix(x, 3);
            uint256 c = mix(x, 5);
            uint256 d = mix(x, 7);
            uint256 e = mix(x, 11);
            uint256 f = mix(x, 13);
            uint256 g = mix(x, 17);
            uint256 h = mix(x, 41);
            assembly { mstore(0x40, 0x80) }
            bytes memory buf = new bytes(32);
            return combine(a, b, c, d, e, f, g, h) + a + b + c + d + e + f + g + buf.length;
        }
    }

    function runInternal(uint256 x) external pure returns (uint256) {
        return inner(x);
    }

    function inner(uint256 x) internal pure returns (uint256) {
        unchecked {
            uint256 a = mix(x, 1);
            uint256 b = mix(x, 3);
            uint256 c = mix(x, 5);
            uint256 d = mix(x, 7);
            uint256 e = mix(x, 11);
            uint256 f = mix(x, 13);
            uint256 g = mix(x, 17);
            uint256 h = mix(x, 41);
            assembly { mstore(0x40, 0x80) }
            bytes memory buf = new bytes(32);
            return combine(a, b, c, d, e, f, g, h) + a + b + c + d + e + f + g + buf.length;
        }
    }

    function mix(uint256 x, uint256 y) internal pure returns (uint256) {
        unchecked {
            return x * y + 1;
        }
    }

    function combine(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f, uint256 g, uint256 h)
        internal
        pure
        returns (uint256)
    {
        unchecked {
            return a + 2 * b + 3 * c + 4 * d + 5 * e + 6 * f + 7 * g + 8 * h;
        }
    }
}
