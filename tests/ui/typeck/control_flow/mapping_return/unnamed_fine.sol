// ported-from: test/libsolidity/syntaxTests/controlFlow/mappingReturn/unnamed_fine.sol
contract C {
    mapping(uint=>uint) m;
    function f() internal view returns (mapping(uint=>uint) storage) { return m; }
}
