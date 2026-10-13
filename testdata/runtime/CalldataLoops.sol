// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

// Loops over calldata arrays as batch entry points read them: totals, weighted
// totals, maxima, searches and dot products. Each call repeats its pass
// `rounds` times, so execution rather than the calldata cost sets the gas used.
contract CalldataLoops {
    function sum(uint256[] calldata amounts, uint256 rounds) external pure returns (uint256 total) {
        for (uint256 r = 0; r < rounds; ++r) {
            for (uint256 i = 0; i < amounts.length; ++i) {
                total += amounts[i];
            }
        }
    }

    function weighted(uint256[] calldata amounts, uint256 price, uint256 rounds)
        external
        pure
        returns (uint256 total)
    {
        for (uint256 r = 0; r < rounds; ++r) {
            for (uint256 i = 0; i < amounts.length; ++i) {
                total += amounts[i] * price;
            }
        }
    }

    function max(uint256[] calldata amounts, uint256 rounds) external pure returns (uint256 best) {
        for (uint256 r = 0; r < rounds; ++r) {
            for (uint256 i = 0; i < amounts.length; ++i) {
                uint256 amount = amounts[i];
                if (amount > best) best = amount;
            }
        }
    }

    function indexOf(address[] calldata accounts, address account, uint256 rounds)
        external
        pure
        returns (uint256 found)
    {
        for (uint256 r = 0; r < rounds; ++r) {
            found = accounts.length;
            for (uint256 i = 0; i < accounts.length; ++i) {
                if (accounts[i] == account) {
                    found = i;
                    break;
                }
            }
        }
    }

    function dot(uint256[] calldata a, uint256[] calldata b, uint256 rounds)
        external
        pure
        returns (uint256 total)
    {
        require(a.length == b.length, "length");
        for (uint256 r = 0; r < rounds; ++r) {
            for (uint256 i = 0; i < a.length; ++i) {
                total += a[i] * b[i];
            }
        }
    }
}
