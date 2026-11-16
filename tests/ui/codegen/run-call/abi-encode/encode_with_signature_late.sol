//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: storageString => true
//@ run-call: storageBytes => true
//@ run-call: convertedStorageString => true
//@ run-call: memoryString => true
//@ run-call: unchanged => true
//@ run-call: literal => true
// The signature of `abi.encodeWithSignature` is read when the selector is hashed, after the
// arguments ran, as solc does, legacy and via-IR.
contract Test {
    string text = "oldsig()";
    bytes blob = "oldsig()";

    function writeText() internal returns (uint256) {
        text = "newsig()";
        blob = "newsig()";
        return 0;
    }

    function overwrite(string memory s) internal pure returns (uint256) {
        bytes(s)[0] = "n";
        return 0;
    }

    function storageString() external returns (bool) {
        bytes memory got = abi.encodeWithSignature(text, writeText());
        return keccak256(got) == keccak256(abi.encodeWithSignature("newsig()", 0));
    }

    function storageBytes() external returns (bool) {
        bytes memory got = abi.encodeWithSignature(string(blob), writeText());
        return keccak256(got) == keccak256(abi.encodeWithSignature("newsig()", 0));
    }

    function convertedStorageString() external returns (bool) {
        bytes memory got = abi.encodeWithSignature(string(bytes(text)), writeText());
        return keccak256(got) == keccak256(abi.encodeWithSignature("newsig()", 0));
    }

    function memoryString() external pure returns (bool) {
        string memory s = "oldsig()";
        bytes memory got = abi.encodeWithSignature(s, overwrite(s));
        return keccak256(got) == keccak256(abi.encodeWithSignature("nldsig()", 0));
    }

    function unchanged() external view returns (bool) {
        bytes memory got = abi.encodeWithSignature(text, uint256(1));
        return keccak256(got) == keccak256(abi.encodeWithSignature("oldsig()", 1));
    }

    function literal() external returns (bool) {
        bytes memory got = abi.encodeWithSignature("oldsig()", writeText());
        return keccak256(got) == keccak256(abi.encodeWithSignature("oldsig()", 0));
    }
}
