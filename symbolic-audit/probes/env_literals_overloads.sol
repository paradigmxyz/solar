interface IFoo { function foo(uint256) external; function bar() external view returns (bool); }
contract EnvLiteralsOverloads {
    function msgSig() external pure returns (bytes4) { return msg.sig; }
    function msgDataLen(uint256, bytes calldata) external pure returns (uint256) { return msg.data.length; }
    function msgDataHash(uint256 x) external pure returns (bytes32) { x; return keccak256(msg.data); }
    function msgDataSlice(uint256 x) external pure returns (bytes memory) { x; return msg.data[4:]; }
    function msgValue() external payable returns (uint256) { return msg.value; }
    function env() external view returns (uint256, uint256, uint256, uint256, address, address, uint256, uint256, uint256) { return (block.number, block.timestamp, block.chainid, block.basefee, block.coinbase, tx.origin, tx.gasprice, block.gaslimit, block.prevrandao); }
    function blobs() external view returns (uint256, bytes32) { return (block.blobbasefee, blobhash(0)); }
    function typeInfo() external pure returns (string memory, bytes4, uint256, int256, int8, uint8) { return (type(EnvLiteralsOverloads).name, type(IFoo).interfaceId, type(uint256).max, type(int256).min, type(int8).max, type(uint8).min); }
    function strEsc() external pure returns (bytes memory) { return bytes("a\tb\nc\\d\"e\x41\x00f"); }
    function strUni() external pure returns (bytes memory) { return bytes(unicode"héllo ✓ 🚀"); }
    function strHex() external pure returns (bytes memory) { return hex"00_01_ff"; }
    function strConcatLit() external pure returns (bytes memory) { return bytes("ab" "cd" 'ef'); }
    function strLong() external pure returns (uint256, bytes32) { string memory s = "0123456789012345678901234567890123456789012345678901234567890123456789"; return (bytes(s).length, keccak256(bytes(s))); }
    function strEmpty() external pure returns (uint256, bytes memory) { return (bytes("").length, bytes("")); }
    function b32Lit() external pure returns (bytes32) { return "abc"; }
    function b3Lit() external pure returns (bytes3) { return "abc"; }
    function addrLit() external pure returns (address) { return 0x5B38Da6a701c568545dCfcB03FcB875f56beddC4; }
    function numLits() external pure returns (uint256, uint256, uint256, int256, uint256) { return (1_000_000, 0xff, 1e3, -1e2, 2.5e3); }
    function unitLits() external pure returns (uint256, uint256, uint256, uint256, uint256) { return (1 wei, 1 gwei, 1 ether, 1 minutes, 1 weeks); }
    function ov(uint256 x) internal pure returns (uint256) { return 2 + x * 0; }
    function ov(int256 x) internal pure returns (uint256) { return 3 + uint256(x) * 0; }
    function ov(bytes memory x) internal pure returns (uint256) { return 4 + x.length * 0; }
    function ov(address x) internal pure returns (uint256) { return 6 + uint256(uint160(x)) * 0; }
    function ov(bool x) internal pure returns (uint256) { return 7 + (x ? 0 : 0); }
    function overloads(uint256 b, int256 c, address d, bool e) external pure returns (uint256) { return ov(b) * 10 + ov(c) * 100 + ov(d) * 1000 + ov(e) * 10000 + ov(bytes(hex"01")) * 1000000; }
    function overloadLit() external pure returns (uint256) { return ov(uint256(1)) * 10 + ov(int256(-1)) * 100 + ov(true) * 1000 + ov(address(0)) * 10000; }
    function sel() external pure returns (bytes4, bytes4, bytes4) { return (this.overloads.selector, IFoo.foo.selector, IFoo.bar.selector); }
    function encodeCallOv(uint8 x) external view returns (bytes memory) { return abi.encodeCall(this.msgDataLen, (x, hex"aa")); }
}
