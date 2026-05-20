//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionCalls/calloptions_on_delegatecall.sol
// ported-from: test/libsolidity/syntaxTests/functionCalls/calloptions_on_staticcall.sol
// ported-from: test/libsolidity/smtCheckerTests/external_calls/external_call_with_gas_1.sol

contract C {
    function valid(address addr, bytes memory data) public payable {
        (bool s1, bytes memory r1) = addr.call{value: 1, gas: 1000}(data);
        (bool s2, bytes memory r2) = addr.delegatecall{gas: 1000}(data);
        (bool s3, bytes memory r3) = addr.staticcall{gas: 1000}(data);
        s1; r1; s2; r2; s3; r3;
    }

    function invalid(address addr, bytes memory data) public {
        addr.delegatecall{value: 1, gas: 1000}(data); //~ ERROR: cannot set option `value` for delegatecall
        addr.staticcall{value: 1, gas: 1000}(data); //~ ERROR: cannot set option `value` for staticcall
    }
}
