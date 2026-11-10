//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: noArguments => 42
//@ run-call: wordArgument 17 => 17
//@ run-call: returningWord 17 => 18
//@ run-call: arrayArgument => 3
import {Return} from "solar:core/Return.sol";

contract Test {
    // Merely keeping these pointers must not attach their exits to incompatible calls.
    function(bytes memory) internal pure returns (uint256) bytesExit = rawBytes;
    function(bool) internal pure returns (uint256) boolExit = rawBool;
    function(uint256) internal pure wordExit = rawWord;
    function() internal pure returns (bool) boolReturnExit = rawBoolReturn;

    function noArguments() public view returns (uint256) {
        // A pure target remains reachable through a view pointer with the same signature.
        function() internal view f = noop;
        f();
        return 42;
    }

    function wordArgument(uint256 x) public pure returns (uint256) {
        function(uint256) internal pure returns (uint256) f = increment;
        return f(x) - 1;
    }

    function returningWord(uint256 x) public pure returns (uint256) {
        function(uint256) internal pure returns (uint256) f = increment;
        return f(x);
    }

    function arrayArgument() public pure returns (uint256) {
        function(uint256[] memory) internal pure returns (uint256) f = arrayLength;
        return f(new uint256[](3));
    }

    function noop() internal pure {}
    function increment(uint256 x) internal pure returns (uint256) { return x + 1; }
    function arrayLength(uint256[] memory a) internal pure returns (uint256) { return a.length; }
    function rawBytes(bytes memory data) internal pure returns (uint256 result) { Return.raw(data); }
    function rawBool(bool flag) internal pure returns (uint256 result) { Return.raw(abi.encode(flag)); }
    function rawWord(uint256 x) internal pure { Return.raw(abi.encode(x)); }
    function rawBoolReturn() internal pure returns (bool result) { Return.raw(hex"01"); }
}
