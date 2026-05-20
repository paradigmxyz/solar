//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/libraries/library_function_selectors.sol
// ported-from: test/libsolidity/smtCheckerTests/function_selector/function_selector_via_contract_name.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/external_library_function_to_external_function_type.sol

type Pointer is uint256;

library PointerLib {
    function offset(Pointer ptr, uint256 by) internal pure returns (Pointer next) {
        ptr;
        by;
    }

    function select(Pointer ptr) external pure returns (Pointer next) {
        return ptr;
    }
}

interface Executor {
    function execute(uint256 value) external returns (bytes4 magic);
}

contract C {
    function libraryFunctionPointer() public pure {
        function(Pointer, uint256) internal pure returns (Pointer) fn = PointerLib.offset;
        fn;
    }

    function interfaceFunctionSelector() public pure returns (bytes4) {
        return Executor.execute.selector;
    }

    function libraryFunctionSelector() public pure returns (bytes4) {
        return PointerLib.select.selector;
    }

    function run(function(Pointer) external pure returns (Pointer) fn) internal pure {}

    function externalLibraryFunctionIsSpecial() public pure {
        run(PointerLib.select); //~ ERROR: mismatched types
        function(Pointer) external pure returns (Pointer) fn = PointerLib.select; //~ ERROR: mismatched types
    }
}
