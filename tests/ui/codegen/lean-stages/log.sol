//@ compile-flags: --emit=bin-runtime -Zemit-lean-stages=../../target/ui-lean-stages
// Logs with every topic count are effects of both layers, so their stages export.

contract LeanStagesLog {
    event E0(uint256) anonymous;
    event E1(uint256);
    event E2(uint256 indexed, uint256);
    event E3(uint256 indexed, uint256 indexed, uint256);
    event E4(uint256 indexed, uint256 indexed, uint256 indexed, uint256);

    function f(uint256 a) external {
        emit E0(a);
        emit E1(a);
        emit E2(a, a);
        emit E3(a, a, a);
        emit E4(a, a, a, a);
    }
}
