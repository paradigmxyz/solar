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
    string.concat(value); //~ ERROR: `string.concat` arguments must be strings
    string.concat(data); //~ ERROR: `string.concat` arguments must be strings

    bytes.concat(data, word);
    bytes.concat(value); //~ ERROR: `bytes.concat` arguments must be bytes or fixed bytes
    bytes.concat(signature); //~ ERROR: `bytes.concat` arguments must be bytes or fixed bytes

    abi.encode(selector, signature, data, word, value);
    abi.encode(uint256); //~ ERROR: argument cannot be ABI-encoded
    abi.encode(type(uint256)); //~ ERROR: argument cannot be ABI-encoded

    abi.encodePacked(selector, signature, data, word, value);
    abi.encodePacked(uint256); //~ ERROR: argument cannot be ABI-encoded
    abi.encodePacked(type(uint256)); //~ ERROR: argument cannot be ABI-encoded

    abi.encodeWithSelector(selector, signature, data, word, value);
    abi.encodeWithSelector(signature, data); //~ ERROR: mismatched types
    abi.encodeWithSelector(selector, uint256); //~ ERROR: argument cannot be ABI-encoded

    abi.encodeCall(Target.target, (value, word));
    abi.encodeCall(value, (value, word)); //~ ERROR: first argument to `abi.encodeCall` must be a function
    abi.encodeCall(Target.target, (signature, word)); //~ ERROR: mismatched types

    abi.encodeWithSignature(signature, selector, data, word, value);
    abi.encodeWithSignature(selector, data); //~ ERROR: mismatched types
    abi.encodeWithSignature(signature, uint256); //~ ERROR: argument cannot be ABI-encoded

    uint256 single = abi.decode(data, (uint256));
    abi.decode(signature, (uint256)); //~ ERROR: mismatched types
    abi.decode(data, (value)); //~ ERROR: `abi.decode` type tuple components must be types

    (uint256 a, bool b) = abi.decode(data, (uint256, bool));
    abi.decode(data, (uint256, value)); //~ ERROR: `abi.decode` type tuple components must be types
    abi.decode(data, (uint256, 1)); //~ ERROR: `abi.decode` type tuple components must be types

    bytes memory decoded = abi.decode(data, (bytes));
    abi.decode(data, (bytes, value)); //~ ERROR: `abi.decode` type tuple components must be types
    abi.decode(data, (bytes, 1)); //~ ERROR: `abi.decode` type tuple components must be types
}
