//@ revisions: unoptimized optimized
//@[unoptimized] compile-flags: -O none
//@[optimized] compile-flags: -O gas
//@[unoptimized] run-call: slice(string,uint256) "abcd", 0 => "abcd"
//@[unoptimized] run-call: slice(string,uint256) "abcd", 2 => "abcd"
//@[unoptimized] run-call: slice(string,uint256) "abcd", 4 => "abcd"
//@[unoptimized] run-call-fail: slice(string,uint256) "abcd", 5 => 0x
//@[unoptimized] run-call: segment(string,uint256,uint256) "abcd", 1, 3 => "bc"
//@[unoptimized] run-call: segment(string,uint256,uint256) "abcd", 4, 4 => ""
//@[optimized] run-call: slice(string,uint256) "abcd", 0 => "abcd"
//@[optimized] run-call: slice(string,uint256) "abcd", 2 => "abcd"
//@[optimized] run-call: slice(string,uint256) "abcd", 4 => "abcd"
//@[optimized] run-call-fail: slice(string,uint256) "abcd", 5 => 0x
//@[optimized] run-call: segment(string,uint256,uint256) "abcd", 1, 3 => "bc"
//@[optimized] run-call: segment(string,uint256,uint256) "abcd", 4, 4 => ""
// ported-from: test/libsolidity/semanticTests/strings/concat/string_concat_different_types.sol

contract CalldataStringSlicing {
    function slice(string calldata value, uint256 split)
        external
        pure
        returns (string memory)
    {
        return string.concat(value[:split], value[split:]);
    }

    function segment(string calldata value, uint256 start, uint256 end)
        external
        pure
        returns (string memory)
    {
        string calldata selected = value[start:end];
        return selected;
    }
}
