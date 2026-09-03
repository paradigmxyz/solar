//@ codegen-matrix: standard
//@ run-call: newLength 0x101 => 1
//@ run-call: newBytesLength 0x101 => 1
//@ run-call: mappingKey 0x101 => 7
//@ run-call: signedMappingKey 0x1ff => 7

contract ImplicitWidenAllocMapping {
    mapping(uint256 => uint256) m;
    mapping(int256 => uint256) mi;

    function inject(uint256 raw) internal pure returns (uint8 x) {
        assembly {
            x := raw
        }
    }

    function injectSigned(uint256 raw) internal pure returns (int8 x) {
        assembly {
            x := raw
        }
    }

    function newLength(uint256 raw) external pure returns (uint256) {
        uint256[] memory a = new uint256[](inject(raw));
        return a.length;
    }

    function newBytesLength(uint256 raw) external pure returns (uint256) {
        bytes memory b = new bytes(inject(raw));
        return b.length;
    }

    function mappingKey(uint256 raw) external returns (uint256) {
        m[1] = 7;
        return m[inject(raw)];
    }

    function signedMappingKey(uint256 raw) external returns (uint256) {
        mi[-1] = 7;
        return mi[injectSigned(raw)];
    }
}
