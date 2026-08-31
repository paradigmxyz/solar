//@ revisions: gas size
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: LargeWordArray::hash() => 0xa41eebc34ef4a2bef7c4463ad5d47685d5cf475712be997e19818707e2e36b43
//@ run-call: SmallWordArray::hash() => 0x413fb8809e5063fd99860bceb9963617ce9a04f9001f7825ffacb57e3d64f140
//@ run-call: NestedStructArray::hash() => 0x267950286dfa1667d06ecbbef2341c0ff0300e2241889da8186dbbc3256b3dbe
//@ run-call: NullableFixedArray::hash(bool) false => 0xbd25874911abf23663dfdb4f360566b8be904042a4b73297ff91a2ed4b0b9290
//@ run-call: NullableFixedArray::hash(bool) true => 0xe024dd41efb1448109e8a893545763be3cc16bbcff9d3c3d657db8f3e979d391
//@ run-call: DynamicFixedArray::returnHash() => 0xae64e4c9ef2c7ec4a51aeb6980efa4df8e8a81def13fea38f8d4a8a76d8607d0
//@ run-call: DynamicFixedArray::encodeHash() => 0xfce20023d5be7adf99be05833ee5b1be38219ef1f717ced202a7139e09572597
//@ run-call: DynamicFixedArray::nestedHash(bool) false => 0xdb666b204e126b7c16cba7510b7a045fd85861c6aff50723985ef5e1198fa055
//@ run-call: DynamicFixedArray::nestedHash(bool) true => 0x81657b39d8099a2aef988f9d70cdb30bcea35a4a9fa1e8ad829ea6a89c1dc354
//@ run-call: DynamicFixedArray::smallHash() => 0x8712a90fa925ee5615802a9a69f58ddcf7da5d8c7eb43a4acb9f0627ef040cbf

contract LargeWordArray {
    function values() external pure returns (bool[989] memory result) {}

    function hash() external view returns (bytes32) {
        (bool success, bytes memory output) =
            address(this).staticcall(abi.encodeCall(this.values, ()));
        require(success);
        return keccak256(output);
    }
}

contract SmallWordArray {
    function values() external pure returns (bool[116] memory result) {
        result[0] = true;
        result[115] = true;
    }

    function hash() external view returns (bytes32) {
        (bool success, bytes memory output) =
            address(this).staticcall(abi.encodeCall(this.values, ()));
        require(success);
        return keccak256(output);
    }
}

contract NestedStructArray {
    struct Value {
        uint256 id;
        bool enabled;
        bytes32[3] words;
    }

    function values() external pure returns (Value[129] memory result) {
        result[0].id = 1;
        result[0].enabled = true;
        result[0].words[2] = bytes32(uint256(2));
        result[128].id = 3;
        result[128].words[0] = bytes32(uint256(4));
    }

    function hash() external view returns (bytes32) {
        (bool success, bytes memory output) =
            address(this).staticcall(abi.encodeCall(this.values, ()));
        require(success);
        return keccak256(output);
    }
}

contract NullableFixedArray {
    function hash(bool populate) external pure returns (bytes32) {
        bool[989][] memory values = new bool[989][](1);
        if (populate) {
            bool[989] memory inner;
            inner[0] = true;
            inner[988] = true;
            values[0] = inner;
        }
        return keccak256(abi.encode(values));
    }
}

contract DynamicFixedArray {
    function values() external pure returns (string[510] memory result) {
        result[0] = "first";
        result[509] = "last";
    }

    function returnHash() external view returns (bytes32) {
        (bool success, bytes memory output) =
            address(this).staticcall(abi.encodeCall(this.values, ()));
        require(success);
        return keccak256(output);
    }

    function encodeHash() external pure returns (bytes32) {
        string[510] memory items;
        items[1] = "one";
        items[508] = "near-last";
        return keccak256(abi.encode(items));
    }

    function nestedHash(bool populate) external pure returns (bytes32) {
        string[510][] memory items = new string[510][](1);
        if (populate) {
            string[510] memory inner;
            inner[0] = "first";
            inner[509] = "last";
            items[0] = inner;
        }
        return keccak256(abi.encode(items));
    }

    function smallHash() external pure returns (bytes32) {
        string[17] memory items;
        items[0] = "first";
        items[16] = "last";
        return keccak256(abi.encode(items));
    }
}
