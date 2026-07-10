//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionCalls/calloptions_on_delegatecall.sol
// ported-from: test/libsolidity/syntaxTests/functionCalls/calloptions_on_staticcall.sol
// ported-from: test/libsolidity/smtCheckerTests/external_calls/external_call_with_gas_1.sol

contract Delegatecall {
    function foo() internal pure {
        address(10).delegatecall{value: 7, gas: 3}("");
        //~^ ERROR: cannot set option `value` for delegatecall
    }
}

contract Staticcall {
    function foo() internal pure {
        address(10).staticcall{value: 7, gas: 3}("");
        //~^ ERROR: cannot set option `value` for staticcall
    }
}

library L {
    function f() public view {
        (bool success,) = address(10).staticcall{gas: 3}("");
        require(success);
    }
}
