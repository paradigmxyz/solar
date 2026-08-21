//@ run-call: C::selectorCompare() => 1
//@ run-call: C::narrowMask() => 1
//@ run-call: C::boolNorm() => 1
//@ run-call: C::signedExtend() => 1
//@ run-call: C::rawScratchReuse() => 1

// Inline assembly assigns raw words into Solidity-typed variables. Reads
// inside assembly keep the raw word (solady-style code shifts a typed
// parameter and reads it back as scratch), while Solidity-level reads
// canonicalize for the declared type, so a `parseSelector`-style extraction
// must not leak the word's low argument bytes into a `bytes4` comparison.

contract C {
    function parseSel(bytes memory b) internal pure returns (bytes4 s) {
        assembly ("memory-safe") {
            s := mload(add(b, 0x20))
        }
    }

    function selectorCompare() external pure returns (uint256) {
        bytes memory data =
            abi.encodeWithSelector(bytes4(0xdc98354e), uint256(7), address(0xBEEF));
        bytes memory only = abi.encode(bytes4(0xdc98354e));
        require(parseSel(data) == parseSel(only), "selector");
        return 1;
    }

    function narrowMask() external pure returns (uint256) {
        uint32 x;
        assembly {
            x := 0x1ffffffff
        }
        require(x == 0xffffffff, "mask");
        return 1;
    }

    function boolNorm() external pure returns (uint256) {
        bool t;
        assembly {
            t := 7
        }
        require(t, "truthy");
        require(t == true, "eq");
        return 1;
    }

    function signedExtend() external pure returns (uint256) {
        int24 v;
        assembly {
            v := 0x800000
        }
        require(v == -8388608, "signext");
        require(v < 0, "neg");
        return 1;
    }

    function firstShiftedByte(address value) internal pure returns (uint256 b) {
        assembly {
            value := shl(96, value)
            b := byte(0, value)
        }
    }

    function rawScratchReuse() external pure returns (uint256) {
        uint256 b = firstShiftedByte(0xA9036907dCcae6a1E0033479B12E837e5cF5a02f);
        require(b == 0xA9, "raw shift");
        return 1;
    }
}
