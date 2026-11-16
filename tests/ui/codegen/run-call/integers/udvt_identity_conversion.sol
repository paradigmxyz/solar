//@ codegen-matrix: standard
//@ run-call: viaWiden 0x101 => 1
//@ run-call: viaWidenSigned 0x100 => 0
//@ run-call: viaWidenSigned 0x1ff => -1

type Small is uint8;
type SignedSmall is int8;

contract UdvtIdentityConversion {
    function inject(uint256 raw) internal pure returns (Small x) {
        assembly {
            x := raw
        }
    }

    function injectSigned(uint256 raw) internal pure returns (SignedSmall x) {
        assembly {
            x := raw
        }
    }

    function widenSmall(Small a) internal pure returns (uint256) {
        return Small.unwrap(a);
    }

    function widenSigned(SignedSmall a) internal pure returns (int256) {
        return SignedSmall.unwrap(a);
    }

    function viaWiden(uint256 raw) external pure returns (uint256) {
        return widenSmall(Small(inject(raw)));
    }

    function viaWidenSigned(uint256 raw) external pure returns (int256) {
        return widenSigned(SignedSmall(injectSigned(raw)));
    }
}
