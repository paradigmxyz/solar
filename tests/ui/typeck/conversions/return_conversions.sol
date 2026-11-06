// ported-from: test/libsolidity/syntaxTests/conversion/implicit_conversion_error_to_bytes4_return_value.sol
// ported-from: test/libsolidity/syntaxTests/conversion/implicit_conversion_event_to_bytes4_return_value.sol
// ported-from: test/libsolidity/syntaxTests/conversion/implicit_conversion_from_string_literal_to_calldata_string.sol
// ported-from: test/libsolidity/syntaxTests/types/address/conversion_error.sol
// ported-from: test/libsolidity/syntaxTests/returnExpressions/single_return_mismatching_number.sol
// ported-from: test/libsolidity/syntaxTests/returnExpressions/tuple_return_mismatching_number.sol

interface I {
    error CustomError(uint256, bool);
}

contract C {
    struct S {
        uint256 value;
    }

    event CustomEvent(uint256);
    uint256[] stateValues;
    S stateValue;

    function validSingle() public pure returns (uint256) {
        return 1;
    }

    function validTuple() public pure returns (uint256, address) {
        return (1, address(0));
    }

    function validEmpty() public pure {
        return;
    }

    function errorToBytes4() public pure returns (bytes4) {
        return I.CustomError; //~ ERROR: mismatched types
    }

    function eventToBytes4() public pure returns (bytes4) {
        return CustomEvent; //~ ERROR: mismatched types
    }

    function stringToCalldata() public pure returns (string calldata) {
        return "hello"; //~ ERROR: mismatched types
    }

    function unicodeToCalldata() public pure returns (string calldata) {
        return unicode"hello"; //~ ERROR: mismatched types
    }

    function bytesToCalldata() public pure returns (bytes calldata) {
        return hex"68656c6c6f"; //~ ERROR: mismatched types
    }

    function negativeLiteralToAddress() public pure returns (address) {
        return -1; //~ ERROR: mismatched types
    }

    function compoundNegativeLiteralToAddress() public pure returns (address) {
        return -(1 + 2); //~ ERROR: mismatched types
    }

    function missingReturnArgument() public pure returns (uint256) {
        return; //~ ERROR: return arguments required
    }

    function tooFewReturnArguments() public pure returns (uint256, uint256) {
        return 1; //~ ERROR: mismatched types
    }

    function tooManyReturnArguments() public pure returns (uint256, uint256) {
        return (1, 2, 3); //~ ERROR: mismatched types
    }

    function memoryToStoragePointer() internal pure returns (uint256[] storage) {
        uint256[] memory values = new uint256[](1);
        return values; //~ ERROR: mismatched types
    }

    function memoryStructToStoragePointer() internal pure returns (S storage) {
        S memory value;
        return value; //~ ERROR: mismatched types
    }

    function validStorageTuple() internal view returns (uint256[] storage, S storage) {
        return (stateValues, stateValue);
    }

    function invalidMixedStorageTuple()
        internal
        view
        returns (uint256[] storage, S storage)
    {
        uint256[] memory values = new uint256[](1);
        return (values, stateValue); //~ ERROR: mismatched types
    }
}
