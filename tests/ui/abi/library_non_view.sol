//@ compile-flags: --emit=abi,hashes --pretty-json

// A library function that can modify state is only reachable through
// `delegatecall`, so solc leaves every library function whose state
// mutability is above `view` out of the JSON ABI. It still lists them in the
// method identifiers. `payable` is rejected for library functions, so
// `nonpayable` is the only mutability this drops in practice.

library L {
    function pureFn(uint256 x) external pure returns (uint256) {
        return x;
    }

    function viewFn() external view returns (uint256) {
        return block.number;
    }

    function nonpayFn(uint256 x) external returns (uint256) {
        assembly {
            sstore(0, x)
        }
        return x;
    }

    function pubFn(uint256 x) public returns (uint256) {
        assembly {
            sstore(1, x)
        }
        return x;
    }
}

// A contract keeps all of them: the rule only applies to libraries.
contract C {
    uint256 x;

    function pureFn(uint256 y) external pure returns (uint256) {
        return y;
    }

    function viewFn() external view returns (uint256) {
        return x;
    }

    function nonpayFn(uint256 y) external returns (uint256) {
        x = y;
        return y;
    }

    function payFn(uint256 y) external payable returns (uint256) {
        x = y;
        return y;
    }
}
