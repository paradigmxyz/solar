// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IPeer {
    function ping() external returns (uint256);
}

contract Peer is IPeer {
    function ping() external pure returns (uint256) {
        return 1;
    }
}

// Creates a sibling, embeds its bytecode, and calls one.
contract Creates {
    function make() external returns (address) {
        return address(new Peer());
    }
}

contract Embeds {
    function code() external pure returns (uint256) {
        return type(Peer).creationCode.length;
    }
}

contract Calls {
    function call(address p) external returns (uint256) {
        return IPeer(p).ping();
    }
}
