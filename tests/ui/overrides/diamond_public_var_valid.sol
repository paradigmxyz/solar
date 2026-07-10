// ported-from: test/libsolidity/syntaxTests/inheritance/override/diamond_top_implemented_intermediate_empty_bottom_public_state_variable.sol

contract Top {
    function f() external view virtual returns (uint) { return 1; }
}
contract Left is Top {}
contract Right is Top {}
contract Bottom is Left, Right {
    uint public override f;
}
