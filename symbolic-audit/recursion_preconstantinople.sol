// Finding 34: any recursive internal function is rejected before constantinople. solc compiles
// this file at every EVM version; we report "EVM IR verification failed: block 6: `push` grows
// the stack to 1025 words" at homestead through byzantium and compile it from constantinople on.
// (With a second, larger function in the contract the default pipeline happens to accept the
// file, so the repro keeps the single recursive function.)
//   solc --bin --via-ir --optimize --evm-version byzantium symbolic-audit/recursion_preconstantinople.sol
//   target/debug/solar --evm-version byzantium --emit bin symbolic-audit/recursion_preconstantinople.sol
//   python3 symbolic-audit/tools/statediff.py symbolic-audit/recursion_preconstantinople.sol C --evm-version byzantium \
//     --fixed "g(uint256) 0" --fixed "g(uint256) 5" --fixed "g(uint256) 300"
contract C {
    function f(uint256 n) internal pure returns (uint256) {
        if (n == 0) return 1;
        return f(n - 1) + 1;
    }
    function g(uint256 n) external pure returns (uint256) { return f(n); }
}
