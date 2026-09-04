//@ revisions: homestead homesteadGas homesteadSize byzantium byzantiumGas byzantiumSize osaka osakaGas osakaSize
//@[homestead] compile-flags: -O none --evm-version homestead
//@[homesteadGas] compile-flags: -O gas --evm-version homestead
//@[homesteadSize] compile-flags: -O size --evm-version homestead
//@[byzantium] compile-flags: -O none --evm-version byzantium
//@[byzantiumGas] compile-flags: -O gas --evm-version byzantium
//@[byzantiumSize] compile-flags: -O size --evm-version byzantium
//@[osaka] compile-flags: -O none --evm-version osaka
//@[osakaGas] compile-flags: -O gas --evm-version osaka
//@[osakaSize] compile-flags: -O size --evm-version osaka
//@ run-call: g 0 => 1
//@ run-call: g 5 => 6
//@ run-call: g 300 => 301
//@ run-call: pingPong 7 => 7
//@ run-call: viaLoop 4 => 10

// A recursive internal call jumps back to the callee's first block with a return address pushed,
// so the callee is entered one word deeper on every round. The stack-depth walk in the EVM IR
// verifier must not accumulate that growth, at any EVM version and any optimization level.
contract C {
    function f(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 1;
        return f(n - 1) + 1;
    }

    function g(uint256 n) external pure returns (uint256) {
        return f(n);
    }

    function ping(uint256 n) internal pure returns (uint256) {
        return n == 0 ? 0 : pong(n - 1) + 1;
    }

    function pong(uint256 n) internal pure returns (uint256) {
        return n == 0 ? 0 : ping(n - 1) + 1;
    }

    // Mutual recursion: the cycle runs through two function bodies.
    function pingPong(uint256 n) external pure returns (uint256) {
        return ping(n);
    }

    // A recursive call inside a loop, so a stack-balanced cycle and a growing one overlap.
    function viaLoop(uint256 n) external pure returns (uint256) {
        uint256 total;
        for (uint256 i; i < n; ++i) {
            total += f(i);
        }
        return total;
    }
}
