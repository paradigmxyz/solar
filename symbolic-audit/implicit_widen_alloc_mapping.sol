// A dirty `uint8` (assigned in assembly) implicitly widened to `uint256` must
// be masked first. solc masks in every context below. solar masks for
// assignment, return, call arguments, and arithmetic, but not when the narrow
// value is used directly as a `new` length or as a wider mapping key.
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

    // 0x101: solc 1, solar 0x101.
    function newLength(uint256 raw) external pure returns (uint256) {
        uint256[] memory a = new uint256[](inject(raw));
        return a.length;
    }

    // 0x101: solc 1, solar 0x101.
    function newBytesLength(uint256 raw) external pure returns (uint256) {
        bytes memory b = new bytes(inject(raw));
        return b.length;
    }

    // 0x101: solc 7 (key 1), solar 0 (key 0x101).
    function mappingKey(uint256 raw) external returns (uint256) {
        m[1] = 7;
        return m[inject(raw)];
    }

    // 0x1ff: solc 7 (key -1 after sign extension), solar 0 (key 0x1ff).
    function signedMappingKey(uint256 raw) external returns (uint256) {
        mi[-1] = 7;
        return mi[injectSigned(raw)];
    }

    // Agrees with solc: the widening goes through a local first.
    function newLengthViaLocal(uint256 raw) external pure returns (uint256) {
        uint256 n = inject(raw);
        uint256[] memory a = new uint256[](n);
        return a.length;
    }

    // Agrees with solc.
    function assign(uint256 raw) external pure returns (uint256) {
        uint256 y = inject(raw);
        return y;
    }
}
