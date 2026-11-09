//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: unchanged => true
//@ run-call: atFreePointer => true
import {Hash} from "solar:core/Hash.sol";
contract Test {
    function unchanged() external pure returns (bool) {
        bytes memory b = hex"11223344";
        uint256 before_;
        uint256 after_;
        assembly { before_ := mload(64) }
        bytes32 result = Hash.keccak256Range(b, 1, 2);
        bytes32 empty = Hash.keccak256Range(b, 4, 0);
        assembly { after_ := mload(64) }
        assert(before_ == after_);
        assert(result == keccak256(hex"2233"));
        assert(empty == keccak256(""));
        return true;
    }
    function atFreePointer() external pure returns (bool) {
        bytes32 expected = keccak256(hex"11223344");
        bytes memory b;
        uint256 before_;
        uint256 after_;
        assembly {
            b := mload(64)
            before_ := b
            mstore(b, 4)
            mstore(add(b, 32), shl(224, 0x11223344))
        }
        bytes32 actual = Hash.keccak256Range(b, 0, 4);
        assembly { after_ := mload(64) }
        assert(before_ == after_);
        assert(actual == expected);
        return true;
    }
}
