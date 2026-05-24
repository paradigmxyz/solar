//@compile-flags: -Ztypeck

interface Target {
    function target(uint256 value, bytes32 word) external;
}

function testVariadicBuiltins(
    bytes4 selector,
    string memory signature,
    string calldata calldataSignature,
    bytes memory data,
    bytes calldata calldataData,
    bytes32 word,
    uint256 value
) pure {
    string.concat("prefix: ", signature, calldataSignature);
    string.concat(value); //~ ERROR: `string.concat` arguments must be strings
    string.concat(data); //~ ERROR: `string.concat` arguments must be strings
    string.concat({a: signature}); //~ ERROR: named arguments cannot be used for functions that take arbitrary parameters

    bytes.concat(data, calldataData, word);
    bytes.concat(value); //~ ERROR: `bytes.concat` arguments must be bytes or fixed bytes
    bytes.concat(signature); //~ ERROR: `bytes.concat` arguments must be bytes or fixed bytes
    bytes.concat({a: data}); //~ ERROR: named arguments cannot be used for functions that take arbitrary parameters

    abi.encode(selector, signature, data, word, value);
    abi.encode(uint256); //~ ERROR: argument cannot be ABI-encoded
    abi.encode(type(uint256)); //~ ERROR: argument cannot be ABI-encoded
    abi.encode({a: value}); //~ ERROR: named arguments cannot be used for functions that take arbitrary parameters

    abi.encodePacked(selector, signature, data, word, value);
    abi.encodePacked(uint256); //~ ERROR: argument cannot be ABI-encoded
    abi.encodePacked(type(uint256)); //~ ERROR: argument cannot be ABI-encoded
    abi.encodePacked({a: value}); //~ ERROR: named arguments cannot be used for functions that take arbitrary parameters

    abi.encodeWithSelector(selector, signature, data, word, value);
    abi.encodeWithSelector(signature, data); //~ ERROR: mismatched types
    abi.encodeWithSelector(selector, uint256); //~ ERROR: argument cannot be ABI-encoded
    abi.encodeWithSelector({selector: selector, a: value}); //~ ERROR: named arguments cannot be used for functions that take arbitrary parameters

    abi.encodeCall(Target.target, (value, word));
    abi.encodeCall(Target.target, value); //~ ERROR: second argument to `abi.encodeCall` must be a tuple
    abi.encodeCall(value, (value, word)); //~ ERROR: first argument to `abi.encodeCall` must be a function
    abi.encodeCall(Target.target, (signature, word)); //~ ERROR: mismatched types
    abi.encodeCall({functionPointer: Target.target, arguments: (value, word)}); //~ ERROR: named arguments cannot be used for functions that take arbitrary parameters

    abi.encodeWithSignature(signature, selector, data, word, value);
    abi.encodeWithSignature(selector, data); //~ ERROR: mismatched types
    abi.encodeWithSignature(signature, uint256); //~ ERROR: argument cannot be ABI-encoded
    abi.encodeWithSignature({signature: signature, a: value}); //~ ERROR: named arguments cannot be used for functions that take arbitrary parameters

    uint256 single = abi.decode(data, (uint256));
    uint256 namedSingle = abi.decode({data: data, types: (uint256)});
    abi.decode(signature, (uint256)); //~ ERROR: mismatched types
    abi.decode(data, (value)); //~ ERROR: `abi.decode` type tuple components must be types

    (uint256 a, bool b) = abi.decode(data, (uint256, bool));
    abi.decode(data, (uint256, value)); //~ ERROR: `abi.decode` type tuple components must be types
    abi.decode(data, (uint256, 1)); //~ ERROR: `abi.decode` type tuple components must be types

    bytes memory decoded = abi.decode(data, (bytes));
    abi.decode(data, (bytes, value)); //~ ERROR: `abi.decode` type tuple components must be types
    abi.decode(data, (bytes, 1)); //~ ERROR: `abi.decode` type tuple components must be types
}

contract VariadicLocations {
    string storedString;
    bytes storedBytes;

    function internalTarget(uint256 value) internal pure {
        value;
    }

    function testConcatLocations(
        string calldata calldataSignature,
        bytes calldata calldataData,
        bytes32 word,
        uint256 value
    ) external {
        string storage storageSignature = storedString;
        string.concat(storedString, storageSignature, calldataSignature);
        string.concat(value); //~ ERROR: `string.concat` arguments must be strings
        string.concat(calldataData); //~ ERROR: `string.concat` arguments must be strings

        bytes storage storageData = storedBytes;
        bytes.concat(storedBytes, storageData, calldataData, calldataData[1:], word);
        bytes.concat(value); //~ ERROR: `bytes.concat` arguments must be bytes or fixed bytes
        bytes.concat(calldataSignature); //~ ERROR: `bytes.concat` arguments must be bytes or fixed bytes
    }

    function testEncodeCallFunctionKinds(uint256 value) external pure {
        abi.encodeCall(internalTarget, (value)); //~ ERROR: first argument to `abi.encodeCall` must be an external function
    }
}
