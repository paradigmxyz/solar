//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: slice(string,uint256) "abcd", 0 => "abcd"
//@ run-call: slice(string,uint256) "abcd", 2 => "abcd"
//@ run-call: slice(string,uint256) "abcd", 4 => "abcd"
//@ run-call-fail: slice(string,uint256) "abcd", 5 => 0x
//@ run-call: segment(string,uint256,uint256) "abcd", 1, 3 => "bc"
//@ run-call: segment(string,uint256,uint256) "abcd", 4, 4 => ""
//@ run-call: nested(string,uint256,uint256,uint256,uint256) "abcdef", 1, 5, 1, 3 => "cd"
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

    function nested(
        string calldata value,
        uint256 outerStart,
        uint256 outerEnd,
        uint256 innerStart,
        uint256 innerEnd
    ) external pure returns (string memory) {
        string calldata selected = value[outerStart:outerEnd][innerStart:innerEnd];
        return selected;
    }
}
