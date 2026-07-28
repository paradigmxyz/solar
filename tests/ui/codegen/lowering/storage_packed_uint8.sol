//@ run-call: setAndGet 1, 2 => 1, 2
//@ run-call: setAKeepsB 255, 2 => 255, 2
//@ run-call: setBKeepsA 255, 0 => 255, 0
//@ run-call: spillCheck => 7, 99, 8
//@ run-call: signedPack -1, 2 => -1, 2
//@ run-call: addressAndUint96 => 0x0000000000000000000000000000000000000001, 42

// Consecutive sub-word state variables share storage slots. A full-word `uint256` starts a new
// slot. The assignments below also check that stores preserve packed neighbors and signed loads
// extend the stored value.

contract PackedU8 {
    uint8 public a;
    uint8 public b;

    function setAndGet(uint8 x, uint8 y) external returns (uint8, uint8) {
        a = x;
        b = y;
        return (a, b);
    }

    function setAKeepsB(uint8 x, uint8 y) external returns (uint8, uint8) {
        a = 0;
        b = y;
        a = x;
        return (a, b);
    }

    function setBKeepsA(uint8 x, uint8 y) external returns (uint8, uint8) {
        a = x;
        b = 0;
        b = y;
        return (a, b);
    }

    uint8 public c;
    uint256 public wide;
    uint8 public d;

    function spillCheck() external returns (uint8, uint256, uint8) {
        c = 7;
        wide = 99;
        d = 8;
        return (c, wide, d);
    }

    int8 public s;
    uint8 public t;

    function signedPack(int8 x, uint8 y) external returns (int8, uint8) {
        s = x;
        t = y;
        return (s, t);
    }

    address public addr;
    uint96 public n;

    function addressAndUint96() external returns (address, uint96) {
        addr = address(1);
        n = 42;
        return (addr, n);
    }
}
