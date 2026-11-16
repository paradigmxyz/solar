//@ codegen-matrix: standard
//@ compile-flags: -Zdebugify=check
//@ normalize-stdout-test: "(?s).+" -> ""
//@ normalize-stderr-test: "debugify: [^\n]*\n" -> ""
//@ run-call: estimate(uint256) 0 => 1
//@ run-call: estimate(uint256) 17 => 4
//@ run-call: estimate(uint256) 0x100000000000000000000000000000000 => 0x10000000000000000
//@ run-call: msb(uint256) 1 => 0
//@ run-call: msb(uint256) 0x8000000000000000000000000000000000000000000000000000000000000000 => 255
// The CLZ form of a branchy search takes the location of the value it replaces, and the
// removed branches leave no location behind.
contract ScanRegionEstimate {
    function estimate(uint256 x) public pure returns (uint256 result) {
        uint256 xAux = x;
        result = 1;
        if (xAux >= 2 ** 128) {
            xAux >>= 128;
            result <<= 64;
        }
        if (xAux >= 2 ** 64) {
            xAux >>= 64;
            result <<= 32;
        }
        if (xAux >= 2 ** 32) {
            xAux >>= 32;
            result <<= 16;
        }
        if (xAux >= 2 ** 16) {
            xAux >>= 16;
            result <<= 8;
        }
        if (xAux >= 2 ** 8) {
            xAux >>= 8;
            result <<= 4;
        }
        if (xAux >= 2 ** 4) {
            xAux >>= 4;
            result <<= 2;
        }
        if (xAux >= 2 ** 2) {
            result <<= 1;
        }
    }

    function msb(uint256 x) public pure returns (uint256 r) {
        unchecked {
            if (x >= 0x100000000000000000000000000000000) {
                x >>= 128;
                r += 128;
            }
            if (x >= 0x10000000000000000) {
                x >>= 64;
                r += 64;
            }
            if (x >= 0x100000000) {
                x >>= 32;
                r += 32;
            }
            if (x >= 0x10000) {
                x >>= 16;
                r += 16;
            }
            if (x >= 0x100) {
                x >>= 8;
                r += 8;
            }
            if (x >= 0x10) {
                x >>= 4;
                r += 4;
            }
            if (x >= 0x4) {
                x >>= 2;
                r += 2;
            }
            if (x >= 0x2) r += 1;
        }
    }
}
