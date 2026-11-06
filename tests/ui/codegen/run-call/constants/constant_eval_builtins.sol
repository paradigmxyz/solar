//@ codegen-matrix: standard
//@[mir] filecheck:
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: hashes => 0x7d87c5ea75f7378bb701e404c50639161af3eff66293e9f375b5f17eb50476f4, 0x7d87c5ea75f7378bb701e404c50639161af3eff66293e9f375b5f17eb50476f4, 0x74f81fe167d99b4cb41d6d0ccda82278caee9f3e2f25d5e5a3936ff3dcec60d0, 0xeb825c4b24f425077a067cc3bef457783f5ad705
//@ run-call: encodings => 0x76b1d8c11f5c19046f65ec154543d0478f890d7041548aad3a676dbd92f75cca, 0x823415a046702e3f6b9dfa7830819c1db403dde6e040ac03030f89659c8a28c1, 0xa3190bd25c55af70e0937190363870691bb145709615483b41806df602076936, 0xa3190bd25c55af70e0937190363870691bb145709615483b41806df602076936, 0xc69d3767974d3245b282a06839bd816094b8f1dd882603658a17daa8b41ac944
//@ run-call: joined => "solar-solar"
//@ run-call: slots => 66579308591070248346498802425823584103384947830069092647693768980449620390400, 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc
//@ run-call: arithmetic => 5, 43, 2, 255
//@ run-call: bytesOps => 0x345678ff, 0x03, 5, 20
//@ run-call: conversions => 255, -56, 16909060, 0x000000000000000000000000000000000000dEaD, 0x000000000000000000000000000000000000beef, 51966, 1
//@ run-call: typeInfo => -32768, 32767, 0, 0xc0e668ec, 0x0dbe671f, 0x3df743fd, 0xb30c6e011c2816674a43c08a53579e48c30e2158e65c09ae36edd17363863f98
//@ run-call: contractName => "ConstantEvalBuiltins"

// Constant evaluation folds pure builtins and conversions of constant operands, so each of these
// functions returns an immediate. The expected values come from solc.

interface I {
    function a() external;
    function b(uint256) external;
}

enum E {
    A,
    B,
    C
}

type U is uint64;

contract ConstantEvalBuiltins {
    event Ev(uint256 indexed value);
    error Err(address account);

    bytes constant DATA = hex"0102030405";
    string constant NAME = "solar";
    bytes32 constant HASH = keccak256(DATA);
    bytes32 constant SHA = sha256(DATA);
    bytes20 constant RIPEMD = ripemd160(DATA);
    bytes constant ENCODED = abi.encode(uint8(1), true, address(0xdEaD), DATA);
    bytes constant PACKED = abi.encodePacked(uint16(0x1234), bytes2(0xabcd), NAME);
    bytes constant WITH_SELECTOR = abi.encodeWithSelector(I.b.selector, 7);
    bytes constant WITH_SIGNATURE = abi.encodeWithSignature("b(uint256)", 7);
    bytes constant CONCAT = bytes.concat(bytes4(0xdeadbeef), DATA, "!");
    string constant JOINED = string.concat(NAME, "-", NAME);
    uint256 constant SLOT = erc7201("solar.main");
    bytes32 constant ERC1967_SLOT = bytes32(uint256(keccak256("eip1967.proxy.implementation")) - 1);
    uint256 constant MODS = addmod(type(uint256).max, 2, 7) + mulmod(type(uint256).max, 3, 11);
    U constant WRAPPED = U.wrap(42);
    uint64 constant UNWRAPPED = U.unwrap(WRAPPED) + 1;
    E constant LAST = type(E).max;
    uint8 constant LAST_INDEX = uint8(LAST);
    bytes4 constant SHIFTED = bytes4(0x12345678) << 8 | ~bytes4(0xffffff00);
    uint8 constant TERNARY = (HASH != 0 ? 250 : 0) + 5;

    // CHECK-LABEL: fn @hashes()
    // CHECK-NOT: keccak256
    // CHECK: ret
    function hashes() external pure returns (bytes32, bytes32, bytes32, bytes20) {
        return (HASH, keccak256(abi.encodePacked(DATA)), SHA, RIPEMD);
    }

    // CHECK-LABEL: fn @encodings()
    // CHECK-NOT: keccak256
    // CHECK: ret
    function encodings() external pure returns (bytes32, bytes32, bytes32, bytes32, bytes32) {
        return (
            keccak256(ENCODED),
            keccak256(PACKED),
            keccak256(WITH_SELECTOR),
            keccak256(WITH_SIGNATURE),
            keccak256(CONCAT)
        );
    }

    function joined() external pure returns (string memory) {
        return JOINED;
    }

    // CHECK-LABEL: fn @slots()
    // CHECK-NOT: keccak256
    // CHECK: ret
    function slots() external pure returns (uint256, bytes32) {
        return (SLOT, ERC1967_SLOT);
    }

    // CHECK-LABEL: fn @arithmetic()
    // CHECK-NOT: {{addmod|mulmod}}
    // CHECK: ret
    function arithmetic() external pure returns (uint256, uint64, uint8, uint8) {
        return (MODS, UNWRAPPED, LAST_INDEX, TERNARY);
    }

    // CHECK-LABEL: fn @bytesOps()
    // CHECK-NOT: shl
    // CHECK: ret
    function bytesOps() external pure returns (bytes4, bytes1, uint256, uint8) {
        return (SHIFTED, DATA[2], DATA.length, bytes20(0).length);
    }

    // CHECK-LABEL: fn @conversions()
    // CHECK-NOT: {{and|shl|shr}}
    // CHECK: ret
    function conversions()
        external
        pure
        returns (uint8, int8, uint32, address, bytes20, uint160, E)
    {
        return (
            uint8(uint16(0x1ff)),
            int8(uint8(200)),
            uint32(bytes4(0x01020304)),
            address(uint160(0xdEaD)),
            bytes20(address(0xbEEF)),
            uint160(address(payable(address(0xCafe)))),
            E(1)
        );
    }

    // CHECK-LABEL: fn @typeInfo()
    // CHECK-NOT: icall
    // CHECK: ret
    function typeInfo()
        external
        pure
        returns (int16, int16, uint8, bytes4, bytes4, bytes4, bytes32)
    {
        return (
            type(int16).min,
            type(int16).max,
            uint8(type(E).min),
            type(I).interfaceId,
            I.a.selector,
            Err.selector,
            Ev.selector
        );
    }

    function contractName() external pure returns (string memory) {
        return type(ConstantEvalBuiltins).name;
    }
}
