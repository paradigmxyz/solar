//@ revisions: gas size legacy
//@[gas] compile-flags: -Ogas -Zmir-pipeline=lower-abi,lower-arithmetic,lower-alloc -Zdump=mir
//@[size] compile-flags: -Osize -Zmir-pipeline=lower-abi,lower-arithmetic,lower-alloc -Zdump=mir
//@[legacy] compile-flags: -Ogas --evm-version byzantium -Zmir-pipeline=lower-abi,lower-arithmetic,lower-alloc -Zdump=mir
//@[gas] filecheck: --check-prefixes=CHECK,GAS
//@[size] filecheck: --check-prefixes=CHECK,SIZE
//@[legacy] filecheck: --check-prefixes=CHECK,LEGACY

// A test that a word fits in `k` bits is `x < 2^k` or `x >> k == 0`, and its negation `x > 2^k - 1`
// or `x >> k != 0`. The builder prices both and emits the cheaper one for how the condition is
// read. A check that aborts on a word with a bit above `k` reads the shifted word for free, a
// materialized boolean pays `ISZERO`s, and the comparison pays the push of its limit. So a
// narrowing check is always the shift; a fit that an `and` combines is the comparison for a short
// limit, and the shift where the limit costs more bytes, in size builds from 16 bits on and in gas
// builds from 32. Without shifts the comparison is the only form.
contract FitsInBits {
    // The three words of a static decode combine their fits in one `and`.
    // CHECK-LABEL: fn @decode{{[( ]}}
    // GAS: lt {{v[0-9]+}}, 0x10000
    // GAS: [[HIGH32:v[0-9]+]] = shr {{v[0-9]+}}, 32
    // GAS-NEXT: eq [[HIGH32]], 0
    // GAS: lt {{v[0-9]+}}, 2
    // SIZE: [[HIGH16:v[0-9]+]] = shr {{v[0-9]+}}, 16
    // SIZE-NEXT: eq [[HIGH16]], 0
    // SIZE: [[HIGH32:v[0-9]+]] = shr {{v[0-9]+}}, 32
    // SIZE-NEXT: eq [[HIGH32]], 0
    // SIZE: lt {{v[0-9]+}}, 2
    // LEGACY: lt {{v[0-9]+}}, 0x10000
    // LEGACY: lt {{v[0-9]+}}, 0x100000000
    // LEGACY: lt {{v[0-9]+}}, 2
    function decode(bytes memory data) external pure returns (uint16, uint32, bool) {
        return abi.decode(data, (uint16, uint32, bool));
    }

    // CHECK-LABEL: fn @narrow{{[( ]}}
    // GAS: [[SUM:v[0-9]+]] = add {{v[0-9]+}}, {{v[0-9]+}}
    // GAS-NEXT: [[HIGH:v[0-9]+]] = shr [[SUM]], 16
    // GAS-NEXT: ne [[HIGH]], 0
    // SIZE: [[SUM:v[0-9]+]] = add {{v[0-9]+}}, {{v[0-9]+}}
    // SIZE-NEXT: [[HIGH:v[0-9]+]] = shr [[SUM]], 16
    // SIZE-NEXT: ne [[HIGH]], 0
    // LEGACY: [[SUM:v[0-9]+]] = add {{v[0-9]+}}, {{v[0-9]+}}
    // LEGACY-NEXT: gt [[SUM]], 0xffff
    function narrow(uint16 a, uint16 b) external pure returns (uint16) {
        return a + b;
    }

    // The allocation end is combined with the wrap test of the bump, a boolean in an `or`: the
    // comparison against a literal push in a gas build, the shift where it saves bytes.
    // GAS-LABEL: fn @alloc{{[( ]}}
    // GAS: gt {{v[0-9]+}}, 0xffffffffffffffff
    // SIZE-LABEL: fn @alloc.3{{[( ]}}
    // SIZE: shr {{v[0-9]+}}, 64
    // LEGACY-LABEL: fn @alloc{{[( ]}}
    // LEGACY: gt {{v[0-9]+}}, 0xffffffffffffffff
    function alloc(uint256 n) external pure returns (uint256) {
        uint256[] memory a = new uint256[](n);
        return a.length;
    }
}
