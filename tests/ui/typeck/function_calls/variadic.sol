//@compile-flags: -Ztypeck

interface Target {
    function target(uint256 value, bytes32 word) external;
}

function testVariadicBuiltins(
    bytes4 selector,
    string memory signature,
    bytes memory data,
    bytes32 word,
    uint256 value
) pure {
    string.concat("prefix: ", signature);

    bytes.concat(data, word);

    abi.encode(selector, signature, data, word, value);

    abi.encodePacked(selector, signature, data, word, value);

    abi.encodeWithSelector(selector, signature, data, word, value);

    abi.encodeCall(Target.target, (value, word));

    abi.encodeWithSignature(signature, selector, data, word, value);

    uint256 single = abi.decode(data, (uint256));

    (uint256 a, bool b) = abi.decode(data, (uint256, bool));

    bytes memory decoded = abi.decode(data, (bytes));
}
