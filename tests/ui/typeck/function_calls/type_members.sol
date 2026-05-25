//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/semanticTests/libraries/library_function_selectors.sol
// ported-from: test/libsolidity/smtCheckerTests/function_selector/function_selector_via_contract_name.sol
// ported-from: test/libsolidity/syntaxTests/functionTypes/external_library_function_to_external_function_type.sol
// ported-from: test/libsolidity/syntaxTests/events/event_library_function.sol
// ported-from: test/libsolidity/syntaxTests/abiEncoder/v2_call_to_v2_library_function_pointer_accepting_struct.sol
// ported-from: test/libsolidity/semanticTests/functionTypes/stack_height_check_on_adding_gas_variable_to_function.sol

type Pointer is uint256;

library PointerLib {
    struct Item {
        uint256 x;
    }

    enum Choice {
        A,
        B
    }

    function ping() public {}

    function offset(Pointer ptr, uint256 by) internal pure returns (Pointer next) {
        ptr;
        by;
    }

    function select(Pointer ptr) external pure returns (Pointer next) {
        return ptr;
    }

    function get(Item memory item) external pure returns (Item memory) {
        return item;
    }

    function read(uint256[] storage items) external view returns (uint256) {
        return items.length;
    }

    function mirror(uint256[] memory items) public pure returns (uint256) {
        return items.length;
    }
}

contract C {
    event ExternalFunction(function() external indexed);

    function libraryFunctionPointer() public pure {
        function(Pointer, uint256) internal pure returns (Pointer) fn = PointerLib.offset;
        fn;
    }

    function libraryFunctionSelector() public pure returns (bytes4) {
        return PointerLib.select.selector;
    }

    function libraryFunctionSelectors() public pure returns (bytes4, bytes4, bytes4) {
        return (PointerLib.select.selector, PointerLib.read.selector, PointerLib.mirror.selector);
    }

    function run(function(Pointer) external pure returns (Pointer) fn) internal pure {}

    function externalLibraryFunctionIsSpecial() public pure {
        run(PointerLib.select); //~ ERROR: mismatched types
        function(Pointer) external pure returns (Pointer) fn = PointerLib.select; //~ ERROR: mismatched types
        function(PointerLib.Item memory) external pure returns (PointerLib.Item memory) structFn = PointerLib.get; //~ ERROR: mismatched types
    }

    function eventLibraryFunctionIsSpecial() public {
        emit ExternalFunction(PointerLib.ping); //~ ERROR: mismatched types
    }
}
