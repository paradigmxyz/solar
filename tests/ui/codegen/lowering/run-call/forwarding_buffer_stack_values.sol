//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: NineHarness::run => 1
//@ run-call: FifteenHarness::run => 1

// Values live across `calldatacopy(0, 0, calldatasize())` stay on the stack, since the copy can
// cover every fixed spill slot. Their layouts may use every word that `DUP` reaches, not only the
// first eight.
// https://github.com/paradigmxyz/solar/issues/1625
// The standard matrix's `mir` revision would snapshot the MIR without testing anything the
// runtime calls do not.

contract Nine {
    fallback() external {
        assembly {
            calldatacopy(0x100, 0x20, 0x120)
            let v0 := mload(0x100)
            let v1 := mload(0x120)
            let v2 := mload(0x140)
            let v3 := mload(0x160)
            let v4 := mload(0x180)
            let v5 := mload(0x1a0)
            let v6 := mload(0x1c0)
            let v7 := mload(0x1e0)
            let v8 := mload(0x200)

            calldatacopy(0, 0, calldatasize())
            if calldataload(0) { log1(0, 0, v0) }

            mstore(0, add(add(add(add(add(add(add(add(v0, v1), v2), v3), v4), v5), v6), v7), v8))
            return(0, 0x20)
        }
    }
}

contract NineHarness {
    function run() external returns (uint256) {
        address target = address(new Nine());
        bytes memory values = abi.encode(11, 12, 13, 14, 15, 16, 17, 18, 19);
        for (uint256 pad; pad < 2; pad++) {
            for (uint256 flag; flag < 2; flag++) {
                (bool success, bytes memory result) =
                    target.call(abi.encodePacked(flag, values, new bytes(pad * 512)));
                require(success && abi.decode(result, (uint256)) == 135, "nine");
            }
        }
        return 1;
    }
}

contract Fifteen {
    fallback() external {
        assembly {
            calldatacopy(0x100, 0x20, 0x1e0)
            let v0 := mload(0x100)
            let v1 := mload(0x120)
            let v2 := mload(0x140)
            let v3 := mload(0x160)
            let v4 := mload(0x180)
            let v5 := mload(0x1a0)
            let v6 := mload(0x1c0)
            let v7 := mload(0x1e0)
            let v8 := mload(0x200)
            let v9 := mload(0x220)
            let v10 := mload(0x240)
            let v11 := mload(0x260)
            let v12 := mload(0x280)
            let v13 := mload(0x2a0)
            let v14 := mload(0x2c0)

            calldatacopy(0, 0, calldatasize())
            if calldataload(0) { log1(0, 0, v0) }

            mstore(0, add(add(add(add(add(add(add(add(add(add(add(add(add(add(v0, v1), v2), v3), v4), v5), v6), v7), v8), v9), v10), v11), v12), v13), v14))
            return(0, 0x20)
        }
    }
}

contract FifteenHarness {
    function run() external returns (uint256) {
        address target = address(new Fifteen());
        bytes memory values = abi.encode(11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25);
        for (uint256 pad; pad < 2; pad++) {
            for (uint256 flag; flag < 2; flag++) {
                (bool success, bytes memory result) =
                    target.call(abi.encodePacked(flag, values, new bytes(pad * 512)));
                require(success && abi.decode(result, (uint256)) == 270, "fifteen");
            }
        }
        return 1;
    }
}
