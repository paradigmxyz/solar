// ported-from: test/libsolidity/syntaxTests/controlFlow/mappingReturn/named_fine.sol
contract C {
    mapping(uint=>uint) m;
    function f() internal view returns (mapping(uint=>uint) storage r) { r = m; }
}
