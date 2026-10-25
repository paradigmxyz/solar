//@ codegen-matrix: standard size_paris size_cancun
//@[size_paris] compile-flags: -Osize --evm-version paris
//@[size_cancun] compile-flags: -Osize --evm-version cancun
//@ run-call-fail: onERC721Received 0x0000000000000000000000000000000000000001, 0x0000000000000000000000000000000000000002, 3, 0x => 0x08c379a000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000004150b7a0200000000000000000000000000000000000000000000000000000000
//@ run-call: tail4 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000041122334400000000000000000000000000000000000000000000000000000000
//@ run-call: tail33 => 0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000002111223344556677889900112233445566778899001122334455667788990011223300000000000000000000000000000000000000000000000000000000000000

// A literal revert string is written as stores beside the encoding, and a byte tail is padded with
// zeros even over dirty memory, under every objective and with or without `MCOPY`.
contract LiteralRuntime {
    function onERC721Received(address, address, uint256, bytes calldata) external pure returns (bytes4) {
        revert(string(abi.encodePacked(this.onERC721Received.selector)));
    }

    function tail4() external pure returns (bytes memory) {
        bytes memory value = hex"11223344";
        dirty();
        bytes memory encoded = abi.encode(value);
        require(value.length == 4);
        return encoded;
    }

    function tail33() external pure returns (bytes memory) {
        bytes memory value = hex"112233445566778899001122334455667788990011223344556677889900112233";
        dirty();
        bytes memory encoded = abi.encode(value);
        require(value.length == 33);
        return encoded;
    }

    function dirty() private pure {
        assembly {
            let end := add(mload(0x40), 512)
            for { let p := mload(0x40) } lt(p, end) { p := add(p, 32) } {
                mstore(p, not(0))
            }
        }
    }
}
