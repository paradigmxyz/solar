// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import { Differential } from "./Differential.sol";

/// Differential fuzzing of the optimized Seaport 1.6 (A) against the
/// high-level rewrite (B) on the functions without orders, from fuzzed
/// storage: counters, order status, contract nonces and the guard slot.
contract SimpleFunctionsTest is Differential {
    uint256 internal constant GUARD_SLOT = 0x929eee14;

    function setUp() public {
        _setUpVariants(OPTIMIZED, HIGHLEVEL);
    }

    /// Writes a fuzzed word into each mapping entry the call can read.
    function _seed(address who, bytes32 orderHash, uint256 w0, uint256 w1, uint256 w2, uint256 guard) internal {
        vm.store(seaport, keccak256(abi.encode(who, uint256(0))), bytes32(w0));
        vm.store(seaport, keccak256(abi.encode(orderHash, uint256(1))), bytes32(w1));
        vm.store(seaport, keccak256(abi.encode(who, uint256(2))), bytes32(w2));
        vm.store(seaport, bytes32(GUARD_SLOT), bytes32(guard));
    }

    function testFuzz_getCounter(address caller, address who, uint256 w, uint256 value) public {
        _seed(who, 0, w, 0, 0, 0);
        _diff(caller, value % 3, abi.encodeWithSignature("getCounter(address)", who));
    }

    function testFuzz_getContractOffererNonce(address caller, address who, uint256 w, uint256 value) public {
        _seed(who, 0, 0, 0, w, 0);
        _diff(caller, value % 3, abi.encodeWithSignature("getContractOffererNonce(address)", who));
    }

    function testFuzz_getOrderStatus(address caller, bytes32 h, uint256 w, uint256 value) public {
        _seed(address(0), h, 0, w, 0, 0);
        _diff(caller, value % 3, abi.encodeWithSignature("getOrderStatus(bytes32)", h));
    }

    function testFuzz_information(address caller, uint256 value, uint64 chainId) public {
        if (chainId % 2 == 0) vm.chainId(chainId);
        _diff(caller, value % 3, abi.encodeWithSignature("information()"));
    }

    function testFuzz_name(address caller, uint256 value) public {
        _diff(caller, value % 3, abi.encodeWithSignature("name()"));
    }

    function testFuzz_incrementCounter(address caller, uint256 w, uint256 value, uint64 blockNumber, bytes32 bh) public {
        blockNumber = uint64(bound(blockNumber, 1, type(uint64).max));
        vm.roll(blockNumber);
        vm.setBlockhash(blockNumber - 1, bh);
        _seed(caller, 0, w, 0, 0, 0);
        _diff(caller, value % 3, abi.encodeWithSignature("incrementCounter()"));
    }

    function testFuzz_activateTstore(address caller, uint256 guard, uint256 value) public {
        _seed(address(0), 0, 0, 0, 0, guard % 5);
        _diff(caller, value % 3, abi.encodeWithSignature("__activateTstore()"));
    }

    /// Raw calldata for the simple functions: short, long and malformed.
    function testFuzz_rawSimple(address caller, uint8 which, bytes calldata tail, uint256 value) public {
        bytes4[7] memory sels = [
            bytes4(0xf07ec373), // getCounter(address)
            0xa900866b, // getContractOffererNonce(address)
            0x46423aa7, // getOrderStatus(bytes32)
            0xf47b7740, // information()
            0x06fdde03, // name()
            0x5b34b966, // incrementCounter()
            0x7423eb3c // __activateTstore()
        ];
        _diff(caller, value % 3, abi.encodePacked(sels[which % 7], tail));
    }
}
