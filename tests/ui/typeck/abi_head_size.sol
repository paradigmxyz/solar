// Each head in an ABI encoding, including the heads in the tails of dynamic parts, must stay below
// 2^32 - 1 bytes, since the backend lays out each head word by word. Unlike solc, a struct's head
// counts all its members together. The oversized types sit in dynamic arrays so that they pass the
// memory size check.
contract C {
    struct S1 {
        uint256[134217727] a;
    }
    struct S2 {
        S1 a;
        S1 b;
    }
    struct Dyn {
        S2 s;
        bytes b;
    }

    S2 stored;
    uint256[2 ** 40] big;

    event E(S2[] s); //~ ERROR: type too large for calldata
    error Err(S2[] s); //~ ERROR: type too large for calldata

    function fits(S1 calldata s) external pure {
        s;
    }

    function calldataParam(S2[] calldata s) external pure { //~ ERROR: type too large for calldata
        s;
    }

    function memoryParam(S2[] memory s) external pure { //~ ERROR: type too large for calldata
        s;
    }

    function publicReturn() public pure returns (S2[] memory s) {} //~ ERROR: type too large for calldata

    function dynamicTail(Dyn[] memory d) public pure { //~ ERROR: type too large for calldata
        d;
    }

    function internalParam(S2[] memory s) internal pure returns (bytes memory) {
        return abi.encode(s); //~ ERROR: type too large to ABI-encode
    }

    function encodeStorage() internal view returns (bytes memory) {
        return abi.encodeWithSelector(0x12345678, stored); //~ ERROR: type too large to ABI-encode
    }

    // Packed encoding has no heads.
    function encodeBig() internal view returns (bytes memory, bytes memory) {
        return (abi.encode(big), abi.encodePacked(big)); //~ ERROR: type too large to ABI-encode
    }

    function decode(bytes memory data) internal pure {
        abi.decode(data, (S2)); //~ ERROR: type too large to ABI-decode
    }
}
