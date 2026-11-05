//@ codegen-matrix: standard native
//@[native] compile-flags: -Ogas -Zdump=mir-final
//@[native] filecheck: --check-prefix=NATIVE
//@ run-call: cached 42 => 42
// NATIVE: backend-floor {{[0-9]+}} 0xc0
// `lower-abi` copies `accrue`, which keeps a promoted storage word at an absolute address in its
// local region, into the `accrue.body` that `cached` calls. The heap `cached` places its array in
// must start above that region.
contract C {
    uint256 total;

    function accrue(uint256 n) public returns (bytes memory) {
        for (uint256 i = 0; i < n; i++) total += i + 7;
        return abi.encode(total);
    }

    function cached(uint256 v) external returns (uint256) {
        uint256[1] memory bal;
        bal[0] = v;
        accrue(1);
        return bal[0];
    }
}
