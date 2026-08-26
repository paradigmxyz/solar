//@ compile-flags: -O none --emit=bin

contract YulExtCalls {
    function extCalls(address target) public returns (uint256 result) {
        assembly {
            pop(extcall(target, 0, 0, 0))
            //~^ ERROR: codegen cannot emit EOF-only external calls in legacy bytecode
            pop(extdelegatecall(target, 0, 0))
            //~^ ERROR: codegen cannot emit EOF-only external calls in legacy bytecode
            result := extstaticcall(target, 0, 0)
            //~^ ERROR: codegen cannot emit EOF-only external calls in legacy bytecode
        }
    }
}
