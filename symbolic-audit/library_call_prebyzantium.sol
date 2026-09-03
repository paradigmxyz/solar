// Finding 31: external library calls with return values are rejected before byzantium.
// solc decodes the delegatecall's static return into a fixed output buffer there (no
// RETURNDATASIZE needed); we report "codegen cannot decode linked library returndata before
// Byzantium". Calls without return values compile on both.
//   solc --bin --via-ir --optimize --evm-version homestead symbolic-audit/library_call_prebyzantium.sol
//   target/debug/solar --evm-version homestead --emit bin symbolic-audit/library_call_prebyzantium.sol
library L {
    function dbl(uint256 x) external pure returns (uint256) { return 2 * x; }
    function pair(uint256 x) external pure returns (uint256, uint256) { return (x, x + 1); }
    function noret(uint256 x) external pure { x; }
}
contract C {
    function viaLib(uint256 x) external pure returns (uint256) { return L.dbl(x); }
    function viaLibPair(uint256 x) external pure returns (uint256 a, uint256 b) { (a, b) = L.pair(x); }
    function viaLibNoRet(uint256 x) external pure { L.noret(x); }
}
