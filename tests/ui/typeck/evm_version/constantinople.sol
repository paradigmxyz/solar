//@ revisions: byzantium constantinople
//@[byzantium] compile-flags: --evm-version byzantium
//@[constantinople] compile-flags: --evm-version constantinople
// ported-from: test/libsolidity/syntaxTests/types/address/codehash_before_constantinople.sol
// ported-from: test/libsolidity/syntaxTests/functionCalls/new_with_calloptions_unsupported.sol

contract Created {}

contract C {
    function codehash() public view returns (bytes32) {
        return address(this).codehash;
        //~[byzantium]^ ERROR: builtin `codehash` requires Constantinople-compatible EVM
    }

    function create() public returns (Created) {
        return new Created{salt: bytes32(0)}();
        //~[byzantium]^ ERROR: call option `salt` requires Constantinople-compatible EVM
    }
}
