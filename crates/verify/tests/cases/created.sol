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

/// Deploys pairs and keeps one per unordered pair of tokens: the new pair differs from every
/// pair stored, since each stored one holds code that cannot go away.
/// @custom:invariant forall address x, address y :: getPair[x][y] == getPair[y][x]
/// @custom:invariant forall address x, address y :: getPair[x][y] != address(0) ==> __created[getPair[x][y]]
contract Registry {
    mapping(address => mapping(address => address)) public getPair;

    /// @custom:ensures pair != address(0)
    /// @custom:ensures forall address x, address y :: old(getPair[x][y]) != pair
    function ok_fresh(address a, address b) external returns (address pair) {
        require(a != b && getPair[a][b] == address(0));
        bytes memory bytecode = type(Pair).creationCode;
        bytes32 salt = keccak256(abi.encodePacked(a, b));
        assembly {
            pair := create2(0, add(bytecode, 32), mload(bytecode), salt)
        }
        IPair(pair).initialize(a, b);
        getPair[a][b] = pair;
        getPair[b][a] = pair;
    }

    /// @custom:ensures forall address x, address y :: old(getPair[x][y]) != pair
    function ok_fresh_new(address a, address b) external returns (address pair) {
        require(a != b && getPair[a][b] == address(0));
        pair = address(new Pair());
        getPair[a][b] = pair;
        getPair[b][a] = pair;
    }
}

contract Mortal {
    function kill() external {
        selfdestruct(payable(msg.sender));
    }
}

contract This {
    function ok_this_nonzero() external view {
        assert(address(this) != address(0));
    }

    function ok_sender_nonzero() external view {
        assert(msg.sender != address(0));
    }
}

/// What creation alone establishes, without invariants.
contract Scratch {
    /// Without a call to it, a failed creation leaves 0.
    /// @custom:ensures pair != address(0)
    function bad_unchecked() external returns (address pair) {
        bytes memory bytecode = type(Pair).creationCode;
        assembly {
            pair := create2(0, add(bytecode, 32), mload(bytecode), 0)
        }
    }

    /// @custom:ensures pair != address(0) && __created[pair]
    function ok_immortal() external returns (address pair) {
        pair = address(new Pair());
    }

    /// A contract whose code can remove itself does not join `__created`.
    /// @custom:ensures __created[pair]
    function bad_mortal() external returns (address pair) {
        pair = address(new Mortal());
    }
}
