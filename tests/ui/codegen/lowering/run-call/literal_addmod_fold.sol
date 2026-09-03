//@ codegen-matrix: standard
//@ run-call: test => 0
// ported-from: test/libsolidity/semanticTests/arithmetics/addmod_mulmod.sol

contract LiteralAddmodFold {
    function test() public pure returns (uint256) {
        if ((2**255 + 2**255) % 7 != addmod(2**255, 2**255, 7)) return 1;
        if ((2**255 + 2**255) % 7 != addmod(2**255, 2**255, 7)) return 2;
        return 0;
    }
}
