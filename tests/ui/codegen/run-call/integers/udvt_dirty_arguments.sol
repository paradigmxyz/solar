//@ codegen-matrix: standard
//@ run-call: viaOperator 0x100, 0 => true
//@ run-call: viaCall 0x100, 0 => true
//@ run-call: viaWiden 0x101 => 1
//@ run-call: viaWidenSigned 0x100 => 0

type Small is uint8;
type SignedSmall is int8;

using {eqSmall as ==} for Small global;

function eqSmall(Small a, Small b) pure returns (bool) {
    return Small.unwrap(a) == Small.unwrap(b);
}

contract UdvtDirtyArguments {
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

    function viaOperator(uint256 raw, uint256 raw2) external pure returns (bool) {
        return inject(raw) == inject(raw2);
    }

    function viaCall(uint256 raw, uint256 raw2) external pure returns (bool) {
        return eqSmall(inject(raw), inject(raw2));
    }

    function viaWiden(uint256 raw) external pure returns (uint256) {
        return widenSmall(inject(raw));
    }

    function viaWidenSigned(uint256 raw) external pure returns (int256) {
        return widenSigned(injectSigned(raw));
    }
}
