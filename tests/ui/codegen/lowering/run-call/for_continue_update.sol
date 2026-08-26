//@ codegen-matrix: standard
//@ run-call: ForContinueUpdate::yulLoop() => 2
//@ run-call: ForContinueUpdate::solidityLoop() => 2
//@ run-call-fail: ForContinueUpdate::yulRevert() => 0x8baa579f

contract ForContinueUpdate {
    function yulLoop() external pure returns (uint256 result) {
        assembly {
            for { let i := 0 } lt(i, 3) { i := add(i, 1) } {
                if eq(i, 1) { continue }
                result := add(result, i)
            }
        }
    }

    function solidityLoop() external pure returns (uint256 result) {
        for (uint256 i;; ++i) {
            if (i == 3) break;
            if (i == 1) continue;
            result += i;
        }
    }

    function yulRevert() external pure {
        assembly {
            for {} 1 {
                mstore(0, 0x8baa579f)
                revert(0x1c, 4)
            } { continue }
        }
    }
}
