//@ revisions: london paris
//@[london] compile-flags: --evm-version london
//@[paris] compile-flags: --evm-version paris
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/prevrandao_magic_block_warn_pre_paris.sol
// ported-from: test/libsolidity/syntaxTests/inlineAssembly/difficulty_magic_block_warn_post_paris.sol

contract C {
    function randomness() external view returns (uint256) {
        return block.prevrandao;
        //~[london]^ WARN: `block.prevrandao` is not supported by this EVM version
    }

    function oldDifficulty() external view returns (uint256) {
        return block.difficulty;
        //~[paris]^ WARN: since Paris, `block.difficulty` was replaced by `block.prevrandao`
    }
}
