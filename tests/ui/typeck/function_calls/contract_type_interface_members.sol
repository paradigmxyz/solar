// ported-from: test/libsolidity/smtCheckerTests/function_selector/function_selector_via_contract_name.sol
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

    function ownSelector() external pure returns (bytes4) {
        return OwnOverload.initialize.selector;
    }

    function encodeOwn() external pure returns (bytes memory) {
        return abi.encodeCall(OwnOverload.initialize, (true));
    }

    function hiddenSelector() external pure returns (bytes4) {
        return HiddenOverload.initialize.selector; //~ ERROR: member `initialize` not found
    }

    function inheritedSelector() external pure returns (bytes4) {
        return InheritedOverload.initialize.selector; //~ ERROR: member `initialize` not found
    }
}

contract BaseOverload {
    function initialize(uint value) external {}
}

contract OwnOverload is BaseOverload {
    function initialize(bool value) external {}
}

contract HiddenOverload is BaseOverload {
    function initialize(bool value) internal {}
}

contract InheritedOverload is BaseOverload {}
