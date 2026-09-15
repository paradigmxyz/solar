//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=evm-ir-runtime
//@[gas] filecheck:
//@ run-call: check 0, 0 => true
//@ run-call: check 1, 1 => true
//@ run-call: check 31, 31 => true
//@ run-call: check 32, 32 => true
//@ run-call: check 33, 33 => true
//@ run-call: check 65, 65 => true
//@ run-call: check 1024, 1024 => true
//@ run-call: check 65, 0 => false
//@ run-call: check 65, 31 => false
//@ run-call: check 65, 64 => false
//@ run-call: check 1024, 1023 => false

// CHECK: push 0x8080808080808080808080808080808080808080808080808080808080808080
// CHECK-NEXT: and
// CHECK-NOT: push 0
// CHECK: jumpi
contract ScanExitConstants {
    function check(uint256 length, uint256 badIndex) external pure returns (bool result) {
        require(length <= 1024);
        bytes memory data = new bytes(length);
        for (uint256 i; i < length; ++i) data[i] = i == badIndex ? bytes1(0x80) : bytes1(0x41);
        assembly { mstore(add(add(data, 32), length), 0xdeadbeef) }
        result = scan(data);
        uint256 saved;
        assembly { saved := mload(add(add(data, 32), length)) }
        require(saved == 0xdeadbeef);
    }

    function scan(bytes memory data) internal pure returns (bool result) {
        assembly {
            result := 1
            let length := mload(data)
            if length {
                let cursor := add(data, 32)
                let end := add(cursor, length)
                let saved := mload(end)
                mstore(end, 0)
                for {} 1 {} {
                    if and(mload(cursor), 0x8080808080808080808080808080808080808080808080808080808080808080) {
                        result := 0
                        break
                    }
                    cursor := add(cursor, 32)
                    if iszero(lt(cursor, end)) { break }
                }
                mstore(end, saved)
            }
        }
    }
}
