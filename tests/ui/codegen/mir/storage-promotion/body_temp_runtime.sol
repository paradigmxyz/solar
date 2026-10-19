//@ codegen-matrix: standard
//@ run-call: f 3 => 10
//@ run-call: g 3 => 141
//@ run-call: h 3 => 30
// The call after the loop keeps `f`'s promoted value in its absolute memory
// temporary, and `lower-abi` copies it into the `f.body` that `g` and `h`
// call. The temporary must not overwrite anything the callers keep live.
contract BodyTemp {
    uint256 x;

    function f(uint256 n) public returns (uint256) {
        for (uint256 i; i < n; i++) x = x * 2 + 1;
        (bool ok,) = address(this).staticcall("");
        return ok ? x + n : n;
    }

    function g(uint256 n) external returns (uint256) {
        return f(n) + f(n + 1);
    }

    function h(uint256 n) external returns (uint256) {
        return f(n) * 3;
    }

    fallback() external {}
}
