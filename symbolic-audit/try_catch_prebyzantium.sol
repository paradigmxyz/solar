// Finding 30: try/catch with a bare `catch { }` never compiles before byzantium.
// solc accepts this file at homestead, tangerineWhistle, and spuriousDragon (only the typed
// catch clauses need byzantium); we reject it with an EVM IR verification error because the
// catch path emits RETURNDATACOPY.
//   solc --bin --via-ir --optimize --evm-version homestead symbolic-audit/try_catch_prebyzantium.sol
//   target/debug/solar --evm-version homestead --emit bin symbolic-audit/try_catch_prebyzantium.sol
//   python3 target/symaudit/statediff.py symbolic-audit/try_catch_prebyzantium.sol T --evm-version homestead \
//     --fixed "tryBare(address) 0x0000000000000000000000000000000000000000" --fixed "tryNoRet(address) 0x0000000000000000000000000000000000000000"
interface I {
    function f() external returns (uint256);
    function g() external;
}
contract T {
    function tryBare(address a) external returns (uint256 r) {
        try I(a).f() returns (uint256 v) { r = v; } catch { r = 7; }
    }
    function tryNoRet(address a) external returns (uint256 r) {
        try I(a).g() { r = 1; } catch { r = 7; }
    }
}
