//@ compile-flags: -Ztypeck

// ported-from: test/libsolidity/syntaxTests/types/contractTypeType/members/assign_function_via_contract_name_to_var.sol

interface Executor {
    function execute(uint256 value) external returns (bytes4 magic);
    function check() external pure;
}

contract C {
    function interfaceFunctionSelector() public pure returns (bytes4) {
        return Executor.execute.selector;
    }

    function interfaceFunctionIsDeclaration() public pure {
        function() external pure fn = Executor.check; //~ ERROR: mismatched types
        Executor.check.address; //~ ERROR: member `address` not found
        Executor.check.selector;
    }
}
