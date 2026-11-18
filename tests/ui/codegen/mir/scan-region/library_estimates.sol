//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[size] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@[size] filecheck:
//@ run-call: prbSqrt 0 => 0
//@ run-call: prbSqrt 1 => 1
//@ run-call: prbSqrt 2 => 1
//@ run-call: prbSqrt 3 => 1
//@ run-call: prbSqrt 4 => 2
//@ run-call: prbSqrt 0xf => 3
//@ run-call: prbSqrt 0x10 => 4
//@ run-call: prbSqrt 0x11 => 4
//@ run-call: prbSqrt 0xff => 0xf
//@ run-call: prbSqrt 0x100 => 0x10
//@ run-call: prbSqrt 0x9234567800000000 => 0xc176cbd5
//@ run-call: prbSqrt 0xffffffffffffffffffffffffffffffff => 0xffffffffffffffff
//@ run-call: prbSqrt 0x100000000000000000000000000000000 => 0x10000000000000000
//@ run-call: prbSqrt 0x100000000000000000000000000000000000000000000003039 => 0x10000000000000000000000000
//@ run-call: prbSqrt 0x8000000000000000000000000000000000000000000000000000000000000000 => 0xb504f333f9de6484597d89b3754abe9f
//@ run-call: prbSqrt 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffff
//@ run-call: ozSqrt 0 => 0
//@ run-call: ozSqrt 1 => 1
//@ run-call: ozSqrt 2 => 1
//@ run-call: ozSqrt 3 => 1
//@ run-call: ozSqrt 4 => 2
//@ run-call: ozSqrt 0xf => 3
//@ run-call: ozSqrt 0x10 => 4
//@ run-call: ozSqrt 0x11 => 4
//@ run-call: ozSqrt 0xff => 0xf
//@ run-call: ozSqrt 0x100 => 0x10
//@ run-call: ozSqrt 0x9234567800000000 => 0xc176cbd5
//@ run-call: ozSqrt 0xffffffffffffffffffffffffffffffff => 0xffffffffffffffff
//@ run-call: ozSqrt 0x100000000000000000000000000000000 => 0x10000000000000000
//@ run-call: ozSqrt 0x100000000000000000000000000000000000000000000003039 => 0x10000000000000000000000000
//@ run-call: ozSqrt 0x8000000000000000000000000000000000000000000000000000000000000000 => 0xb504f333f9de6484597d89b3754abe9f
//@ run-call: ozSqrt 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffff
//@ run-call: estimate 0 => 1
//@ run-call: estimate 1 => 1
//@ run-call: estimate 2 => 1
//@ run-call: estimate 3 => 1
//@ run-call: estimate 4 => 2
//@ run-call: estimate 0xf => 2
//@ run-call: estimate 0x10 => 4
//@ run-call: estimate 0x11 => 4
//@ run-call: estimate 0xff => 8
//@ run-call: estimate 0x100 => 0x10
//@ run-call: estimate 0x9234567800000000 => 0x80000000
//@ run-call: estimate 0xffffffffffffffffffffffffffffffff => 0x8000000000000000
//@ run-call: estimate 0x100000000000000000000000000000000 => 0x10000000000000000
//@ run-call: estimate 0x100000000000000000000000000000000000000000000003039 => 0x10000000000000000000000000
//@ run-call: estimate 0x8000000000000000000000000000000000000000000000000000000000000000 => 0x80000000000000000000000000000000
//@ run-call: estimate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x80000000000000000000000000000000
//@ run-call: msb 0 => 0
//@ run-call: msb 1 => 0
//@ run-call: msb 2 => 1
//@ run-call: msb 3 => 1
//@ run-call: msb 4 => 2
//@ run-call: msb 0xf => 3
//@ run-call: msb 0x10 => 4
//@ run-call: msb 0x11 => 4
//@ run-call: msb 0xff => 7
//@ run-call: msb 0x100 => 8
//@ run-call: msb 0x9234567800000000 => 0x3f
//@ run-call: msb 0xffffffffffffffffffffffffffffffff => 0x7f
//@ run-call: msb 0x100000000000000000000000000000000 => 0x80
//@ run-call: msb 0x100000000000000000000000000000000000000000000003039 => 0xc8
//@ run-call: msb 0x8000000000000000000000000000000000000000000000000000000000000000 => 0xff
//@ run-call: msb 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xff
//@ run-call: wrongEstimate 0 => 1
//@ run-call: wrongEstimate 1 => 1
//@ run-call: wrongEstimate 2 => 1
//@ run-call: wrongEstimate 3 => 1
//@ run-call: wrongEstimate 4 => 1
//@ run-call: wrongEstimate 0xf => 2
//@ run-call: wrongEstimate 0x10 => 4
//@ run-call: wrongEstimate 0x11 => 4
//@ run-call: wrongEstimate 0xff => 8
//@ run-call: wrongEstimate 0x100 => 0x10
//@ run-call: wrongEstimate 0x9234567800000000 => 0x80000000
//@ run-call: wrongEstimate 0xffffffffffffffffffffffffffffffff => 0x8000000000000000
//@ run-call: wrongEstimate 0x100000000000000000000000000000000 => 0x10000000000000000
//@ run-call: wrongEstimate 0x100000000000000000000000000000000000000000000003039 => 0x10000000000000000000000000
//@ run-call: wrongEstimate 0x8000000000000000000000000000000000000000000000000000000000000000 => 0x80000000000000000000000000000000
//@ run-call: wrongEstimate 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x80000000000000000000000000000000

// Searches for the highest set bit written with branches that update two values, which
// if-conversion leaves as branches.
contract ScanRegion {
    // CHECK-LABEL: fn @estimate
    // CHECK: clz
    // CHECK-NOT: jumpi
    // CHECK: ret
    function estimate(uint256 x) external pure returns (uint256 result) {
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

    // A step that stops early leaves the estimate wrong for inputs below 16, so the code stays.
    // CHECK-LABEL: fn @wrongEstimate
    // CHECK-NOT: clz
    // CHECK: ret
    function wrongEstimate(uint256 x) external pure returns (uint256 result) {
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
        if (xAux >= 2 ** 3) {
            result <<= 1;
        }
    }

    // PRBMath's `sqrt`.
    // CHECK-LABEL: fn @prbSqrt
    // CHECK: clz
    // CHECK: ret
    function prbSqrt(uint256 x) external pure returns (uint256 result) {
        if (x == 0) {
            return 0;
        }
        uint256 xAux = uint256(x);
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
        unchecked {
            result = (result + x / result) >> 1;
            result = (result + x / result) >> 1;
            result = (result + x / result) >> 1;
            result = (result + x / result) >> 1;
            result = (result + x / result) >> 1;
            result = (result + x / result) >> 1;
            result = (result + x / result) >> 1;
            uint256 roundedResult = x / result;
            if (result >= roundedResult) {
                result = roundedResult;
            }
        }
    }

    // OpenZeppelin's `Math.sqrt`.
    // CHECK-LABEL: fn @ozSqrt
    // CHECK: clz
    // CHECK: ret
    function ozSqrt(uint256 a) external pure returns (uint256) {
        unchecked {
            if (a <= 1) {
                return a;
            }
            uint256 aa = a;
            uint256 xn = 1;
            if (aa >= (1 << 128)) {
                aa >>= 128;
                xn <<= 64;
            }
            if (aa >= (1 << 64)) {
                aa >>= 64;
                xn <<= 32;
            }
            if (aa >= (1 << 32)) {
                aa >>= 32;
                xn <<= 16;
            }
            if (aa >= (1 << 16)) {
                aa >>= 16;
                xn <<= 8;
            }
            if (aa >= (1 << 8)) {
                aa >>= 8;
                xn <<= 4;
            }
            if (aa >= (1 << 4)) {
                aa >>= 4;
                xn <<= 2;
            }
            if (aa >= (1 << 2)) {
                xn <<= 1;
            }
            xn = (3 * xn) >> 1;
            xn = (xn + a / xn) >> 1;
            xn = (xn + a / xn) >> 1;
            xn = (xn + a / xn) >> 1;
            xn = (xn + a / xn) >> 1;
            xn = (xn + a / xn) >> 1;
            xn = (xn + a / xn) >> 1;
            return xn - (xn > a / xn ? 1 : 0);
        }
    }

    // Uniswap v3's `BitMath.mostSignificantBit` without its zero check, unchecked.
    // CHECK-LABEL: fn @msb
    // CHECK: clz
    // CHECK: ret
    function msb(uint256 x) external pure returns (uint256 r) {
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
