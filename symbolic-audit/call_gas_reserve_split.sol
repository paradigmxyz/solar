// Finding 38: at -Ogas the backend shares one CALL block between call sites, so a JUMP and a
// JUMPDEST sit between homestead's `sub(gas(), 50)` and the CALL; the 10-gas margin of the
// reserve (finding 29) is exceeded and a pre-EIP-150 CALL asking for more gas than remains
// throws, consuming everything. Only some call sites in a contract with several shapes hit it.
//   python3 symbolic-audit/tools/prebyz_gas.py symbolic-audit/call_gas_reserve_split.sol R Callee 'live(address)' --evm-version homestead --gas 200000
//   python3 symbolic-audit/tools/prebyz_gas.py symbolic-audit/call_gas_reserve_split.sol R Callee 'livePointer(address)' --evm-version homestead --gas 200000
//   python3 symbolic-audit/tools/prebyz_gas.py symbolic-audit/call_gas_reserve_split.sol R Callee 'liveTwo(address)' --evm-version homestead --gas 200000
interface T {
    function value() external returns (uint256);
    function pair() external returns (uint256, uint256);
    function agg() external returns (uint256[2] memory);
    function noop() external;
    function fail() external returns (uint256);
}
contract Callee {
    function value() external pure returns (uint256) { return 42; }
    function pair() external pure returns (uint256, uint256) { return (1, 2); }
    function agg() external pure returns (uint256[2] memory r) { r[0] = 3; r[1] = 4; }
    function noop() external {}
    function fail() external pure returns (uint256) { revert(); }
}
contract R {
    function live(address a) external returns (uint256 r) { r = T(a).value(); }
    function liveTwo(address a) external returns (uint256 r) { (uint256 x, uint256 y) = T(a).pair(); r = x + y; }
    function liveAggregate(address a) external returns (uint256 r) { uint256[2] memory v = T(a).agg(); r = v[0] + v[1]; }
    function liveNoReturn(address a) external returns (uint256 r) { T(a).noop(); r = 1; }
    function liveCatch(address a) external returns (uint256 r) { r = T(a).fail{gas: 50000}(); }
    function livePointer(address a) external returns (uint256 r) { function() external returns (uint256) f = T(a).value; r = f(); }
    function noCode() external returns (uint256 r) { r = T(address(0)).value(); }
    function noCodeNoReturn() external returns (uint256 r) { T(address(0)).noop(); r = 1; }
}
