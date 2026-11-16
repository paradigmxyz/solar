//@ codegen-matrix: standard
//@ run-call: f() => 1, 2
contract C {
    uint256[] log;
    function t(uint256 k) internal returns (uint256) { log.push(k); return k; }
    function mk() internal returns (uint256[] memory a) { log.push(1); a = new uint256[](3); }
    function f() external returns (uint256, uint256) { mk()[t(2)]; return (log[0], log[1]); }
}
