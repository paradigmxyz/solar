// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Calls into contracts this one creates (UniswapV2's factory deploys pairs with create2 of
// type(Pair).creationCode and initializes them): every ok_* must verify, every bad_* must fail.

interface IPair {
    function initialize(address a, address b) external;
}

contract Pair {
    address public factory;
    address public token0;
    address public token1;

    constructor() {
        factory = msg.sender;
    }

    function initialize(address a, address b) external {
        require(msg.sender == factory);
        token0 = a;
        token1 = b;
    }
}

/// A pair whose initializer calls out: it may reenter.
contract Noisy {
    function initialize(address a, address) external {
        IPair(a).initialize(a, a);
    }
}

contract Factory {
    mapping(address => mapping(address => address)) public getPair;
    address[] public allPairs;

    /// @custom:ensures allPairs.length == old(allPairs.length) + 1
    /// @custom:ensures getPair[a][b] == pair && getPair[b][a] == pair
    function ok_create2(address a, address b) external returns (address pair) {
        bytes memory bytecode = type(Pair).creationCode;
        bytes32 salt = keccak256(abi.encodePacked(a, b));
        assembly {
            pair := create2(0, add(bytecode, 32), mload(bytecode), salt)
        }
        IPair(pair).initialize(a, b);
        getPair[a][b] = pair;
        getPair[b][a] = pair;
        allPairs.push(pair);
    }

    /// @custom:ensures allPairs.length == old(allPairs.length) + 1
    function ok_new(address a, address b) external returns (address pair) {
        uint256 n = allPairs.length;
        pair = address(new Pair());
        IPair(pair).initialize(a, b);
        require(allPairs.length == n);
        allPairs.push(pair);
    }

    /// The initializer calls out, so the call may reenter and change the array.
    /// @custom:ensures allPairs.length == old(allPairs.length) + 1
    function bad_noisy(address a, address b) external returns (address pair) {
        bytes memory bytecode = type(Noisy).creationCode;
        bytes32 salt = 0;
        assembly {
            pair := create2(0, add(bytecode, 32), mload(bytecode), salt)
        }
        IPair(pair).initialize(a, b);
        allPairs.push(pair);
    }

    /// A call to some other address may reenter.
    /// @custom:ensures allPairs.length == old(allPairs.length) + 1
    function bad_other(address a, address b, address other) external returns (address pair) {
        bytes memory bytecode = type(Pair).creationCode;
        bytes32 salt = 0;
        assembly {
            pair := create2(0, add(bytecode, 32), mload(bytecode), salt)
        }
        IPair(other).initialize(a, b);
        allPairs.push(pair);
    }

    /// Outside code ran after the creation, so what the address holds is no longer known.
    /// @custom:ensures allPairs.length == old(allPairs.length) + 1
    function bad_after_outside_call(address a, address b, address other) external returns (address pair) {
        bytes memory bytecode = type(Pair).creationCode;
        bytes32 salt = 0;
        assembly {
            pair := create2(0, add(bytecode, 32), mload(bytecode), salt)
        }
        IPair(other).initialize(a, b);
        IPair(pair).initialize(a, b);
        allPairs.push(pair);
    }

    /// The buffer was changed before the deployment: its code is unknown.
    /// @custom:ensures allPairs.length == old(allPairs.length) + 1
    function bad_patched_code(address a, address b) external returns (address pair) {
        bytes memory bytecode = type(Pair).creationCode;
        bytecode[0] = 0x00;
        bytes32 salt = 0;
        assembly {
            pair := create2(0, add(bytecode, 32), mload(bytecode), salt)
        }
        IPair(pair).initialize(a, b);
        allPairs.push(pair);
    }
}
