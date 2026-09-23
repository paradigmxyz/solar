//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: unsignedSweep 7; gas=16000000 => true
//@ run-call: unsignedSweep 8; gas=16000000 => true
//@ run-call: signedSweep 11; gas=16000000 => true
//@ run-call: neighbours 1234567, 42 => true

// Decimal spellings of every length against a reference that writes the
// digits into a scratch buffer and copies them out, for unsigned and signed
// values, powers of ten and their neighbours, and the extreme values. Two
// spellings in a row, with an allocation after them, keep their own bytes.
import {Strings} from "solar:core/v1/Strings.sol";

contract RoundTrip {
    function spelling(uint256 x) internal pure returns (bytes memory out) {
        bytes memory buffer = new bytes(78);
        uint256 n;
        do {
            buffer[77 - n] = bytes1(uint8(48 + x % 10));
            x /= 10;
            ++n;
        } while (x != 0);
        out = new bytes(n);
        for (uint256 i; i < n; ++i) {
            out[i] = buffer[78 - n + i];
        }
    }

    function signedSpelling(int256 x) internal pure returns (bytes memory) {
        if (x >= 0) return spelling(uint256(x));
        uint256 magnitude = x == type(int256).min ? uint256(1) << 255 : uint256(-x);
        return bytes.concat("-", spelling(magnitude));
    }

    function same(uint256 x) internal pure returns (bool) {
        return keccak256(bytes(Strings.toString(x))) == keccak256(spelling(x));
    }

    function sameSigned(int256 x) internal pure returns (bool) {
        return keccak256(bytes(Strings.toString(x))) == keccak256(signedSpelling(x));
    }

    function unsignedSweep(uint256 seed) public pure returns (bool) {
        for (uint256 i; i < 64; ++i) {
            uint256 x = uint256(keccak256(abi.encode(seed, i))) >> ((i * 37) % 256);
            if (!same(x)) return false;
        }
        uint256 power = 1;
        for (uint256 k; k < 78; ++k) {
            if (!same(power) || !same(power - 1) || !same(power + 1)) return false;
            if (k < 77) power *= 10;
        }
        return same(type(uint256).max);
    }

    function signedSweep(uint256 seed) public pure returns (bool) {
        for (uint256 i; i < 64; ++i) {
            int256 x = int256(uint256(keccak256(abi.encode(seed, i)))) >> ((i * 41) % 256);
            if (!sameSigned(x) || !sameSigned(-(x / 2))) return false;
        }
        return sameSigned(type(int256).min) && sameSigned(type(int256).max) && sameSigned(-1)
            && sameSigned(0) && sameSigned(-10) && sameSigned(9);
    }

    function neighbours(uint256 x, int256 y) public pure returns (bool) {
        string memory a = Strings.toString(x);
        string memory b = Strings.toString(-y);
        bytes memory after_ = new bytes(96);
        for (uint256 i; i < 96; ++i) {
            after_[i] = 0xff;
        }
        return keccak256(bytes(a)) == keccak256(spelling(x))
            && keccak256(bytes(b)) == keccak256(signedSpelling(-y));
    }
}
