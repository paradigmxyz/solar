//@ revisions: default specialized
//@[default] compile-flags: -Zevm-no-specialized-operand-plans
//@[default] run-call: mstorePreserves 7, 512 => 22
//@[specialized] run-call: mstorePreserves 7, 512 => 22
//@[default] run-call: mstore8Preserves 11, 544 => 34
//@[specialized] run-call: mstore8Preserves 11, 544 => 34
//@[default] run-call: sstorePreserves 13 => 40
//@[specialized] run-call: sstorePreserves 13 => 40
//@[default] run-call: tstorePreserves 17 => 52
//@[specialized] run-call: tstorePreserves 17 => 52
//@[default] run-call: mcopyPreserves 19 => 550
//@[specialized] run-call: mcopyPreserves 19 => 550
//@[default] run-call: calldataCopyPreserves 23 => 558
//@[specialized] run-call: calldataCopyPreserves 23 => 558
//@[default] run-call: codeCopyPreserves 29 => 570
//@[specialized] run-call: codeCopyPreserves 29 => 570
//@[default] run-call: extCodeCopyPreserves 31 => 574
//@[specialized] run-call: extCodeCopyPreserves 31 => 574
//@[default] run-call: returnDataCopyPreserves 37 => 586
//@[specialized] run-call: returnDataCopyPreserves 37 => 586

contract SpecializedOperandPlans {
    uint256 private stored;

    function mstorePreserves(uint256 x, uint256 pointer) external pure returns (uint256) {
        uint256 value = x * 2 + 1;
        assembly {
            mstore(pointer, value)
        }
        return value + x;
    }

    function mstore8Preserves(uint256 x, uint256 pointer) external pure returns (uint256) {
        uint256 value = x * 2 + 1;
        assembly {
            mstore8(pointer, value)
        }
        return value + x;
    }

    function sstorePreserves(uint256 x) external returns (uint256) {
        uint256 value = x * 2 + 1;
        assembly {
            sstore(0, value)
        }
        return value + x;
    }

    function tstorePreserves(uint256 x) external returns (uint256) {
        uint256 value = x * 2 + 1;
        assembly {
            tstore(0, value)
        }
        return value + x;
    }

    function mcopyPreserves(uint256 x) external pure returns (uint256) {
        uint256 pointer = x + 0x200;
        assembly {
            mstore(pointer, x)
            mcopy(add(pointer, 32), pointer, 32)
        }
        return pointer + x;
    }

    function calldataCopyPreserves(uint256 x) external pure returns (uint256) {
        uint256 pointer = x + 0x200;
        assembly {
            calldatacopy(pointer, 0, 4)
        }
        return pointer + x;
    }

    function codeCopyPreserves(uint256 x) external pure returns (uint256) {
        uint256 pointer = x + 0x200;
        assembly {
            codecopy(pointer, 0, 4)
        }
        return pointer + x;
    }

    function extCodeCopyPreserves(uint256 x) external view returns (uint256) {
        uint256 pointer = x + 0x200;
        address target = address(this);
        assembly {
            extcodecopy(target, pointer, 0, 4)
        }
        return pointer + x;
    }

    function returnDataCopyPreserves(uint256 x) external returns (uint256) {
        uint256 pointer = x + 0x200;
        (bool success,) = address(this).call(abi.encodeCall(this.noop, (x)));
        require(success);
        assembly {
            returndatacopy(pointer, 0, 0)
        }
        return pointer + x;
    }

    function noop(uint256) external {}
}
