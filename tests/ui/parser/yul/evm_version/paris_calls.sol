//@ revisions: paris london
//@[paris] compile-flags: --evm-version paris
//@[london] compile-flags: --evm-version london
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/prevrandao_nobuitin_pre_paris.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/difficulty_nobuiltin_post_paris.sol

contract C {
    function randomness() external view returns (uint256 result) {
        assembly {
            result := prevrandao()
            //~[london]^ ERROR: unresolved symbol `prevrandao`
        }
    }

    function difficultyValue() external view returns (uint256 result) {
        assembly {
            result := difficulty()
            //~[paris]^ ERROR: unresolved symbol `difficulty`
        }
    }
}
