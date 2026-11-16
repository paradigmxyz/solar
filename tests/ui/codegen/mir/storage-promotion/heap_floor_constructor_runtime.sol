//@ codegen-matrix: standard
//@ run-call: imm; constructor=[1000] => 4660
//@ run-call: res; constructor=[1000] => 2001
// `lower-abi` copies `accrue`, whose three promoted loops keep six words at absolute addresses in
// its local region, into the `accrue.body` that the constructor calls. The constructor's
// immutable staging and heap must start above that region.
contract C {
    uint256 a;
    uint256 b;
    uint256 c;
    uint256 public immutable imm;
    uint256 public res;

    function accrue(uint256 n) public returns (bytes memory) {
        for (uint256 i = 0; i < n; i++) a += i + 7;
        for (uint256 i = 0; i < n; i++) b += i + 8;
        for (uint256 i = 0; i < n; i++) c += i + 9;
        return abi.encode(a + b + c);
    }

    constructor(uint256 v) {
        imm = 0x1234;
        uint256[2] memory bal;
        bal[0] = v;
        bal[1] = v + 1;
        accrue(3);
        res = bal[0] + bal[1];
    }
}
