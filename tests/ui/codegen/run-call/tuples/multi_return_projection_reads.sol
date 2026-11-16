//@ codegen-matrix: standard
//@ filecheck:
//@ run-call: f 1 => 789258
//@ run-call: h 1, 5 => 461842

// Results of a multi-word call read past a `caller` read or an unrelated memory read bind to the
// words the call returns on the stack.
contract C {
    mapping(uint256 => address[4]) rows;
    uint256 total;

    constructor() {
        rows[1] = [address(0xa), address(0xb), address(0xc), msg.sender];
    }

    function resolve(uint256 p) private view returns (address, address, address, address) {
        address[4] storage r = rows[p];
        return (r[0], r[1], r[2], r[3]);
    }

    function mix(uint256 x, address y, uint256 z) private returns (uint256) {
        total += x;
        return x * 0x10000 + uint256(uint160(y)) * 0x100 + z;
    }

    // CHECK-LABEL: fn @f(
    // CHECK: icall @resolve
    // CHECK: caller
    function f(uint256 p) public view returns (uint256) {
        (address a, address b, address s, address o) = resolve(p);
        if (o != msg.sender) revert();
        return uint256(uint160(s)) << 16 | uint256(uint160(b)) << 8 | uint160(a);
    }

    function g(uint256 p) public view returns (address) {
        (address a,,, address o) = resolve(p);
        return o == tx.origin ? a : o;
    }

    // CHECK-LABEL: fn @h(
    // CHECK: icall @resolve
    // CHECK: icall @mix
    function h(uint256 p, uint256 k) public returns (uint256) {
        uint256[4] memory m = [k, k + 1, k + 2, k + 3];
        (address a, address b, address s,) = resolve(p);
        uint256 r = mix(7, s, m[3]);
        if (b != address(0)) r += uint160(a);
        return r;
    }
}
