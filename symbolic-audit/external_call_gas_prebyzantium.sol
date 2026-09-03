// Finding 29: at homestead every external call forwards gas(), which fails before EIP-150.
// Run with target/symaudit/prebyz_gas.py (a forge EVM at homestead, assembly calls only):
//   python3 target/symaudit/prebyz_gas.py symbolic-audit/external_call_gas_prebyzantium.sol R Callee 'callRet(address)' --evm-version homestead
//   python3 target/symaudit/prebyz_gas.py symbolic-audit/external_call_gas_prebyzantium.sol R Callee 'callNoRet(address)' --evm-version homestead
//   python3 target/symaudit/prebyz_gas.py symbolic-audit/external_call_gas_prebyzantium.sol R Callee 'callValue(address)' --evm-version homestead
interface I {
    function f() external returns (uint256);
    function g() external;
    function h() external payable returns (uint256);
}
contract Callee {
    function f() external returns (uint256) { return 42; }
    function g() external {}
    function h() external payable returns (uint256) { return msg.value + 1; }
}
contract R {
    function callRet(address a) external returns (uint256) { return I(a).f(); }
    function callNoRet(address a) external returns (uint256) { I(a).g(); return 1; }
    function callValue(address a) external returns (uint256) { return I(a).h{value: 0}(); }
    function callPtr(function() external returns (uint256) p) external returns (uint256) { return p(); }
    receive() external payable {}
}
