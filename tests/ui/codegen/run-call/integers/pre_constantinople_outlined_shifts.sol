//@ codegen-matrix: standard byzantium
//@[byzantium] compile-flags: --evm-version byzantium
//@ run-call: test => 2, 1, 1

// Storage `bytes` pushes and pops repeat the same shift-heavy runs, which the
// outliner shares through stubs that return via opaque jumps. Legacy shift
// legalization then needs transient stack headroom without exact depths; the
// backend reserves that budget on every target without native shifts and in
// every optimization mode, so the contract compiles instead of being rejected.
contract PreConstantinopleOutlinedShifts {
    bytes data;

    function test() external returns (uint256 x, uint256 y, uint256 l) {
        data.push(0x07);
        data.push(0x03);
        x = data.length;
        data.pop();
        data.pop();
        data.push(0x02);
        y = data.length;
        l = data.length;
    }
}
