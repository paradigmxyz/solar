// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @notice Checked Solidity sorting without type punning or sentinel reads.
/// @dev In-place memory-length mutation APIs are deliberately absent.
library LibSort {
    function insertionSort(uint256[] memory a) internal pure {
        for (uint256 i = 1; i < a.length; ++i) {
            uint256 value = a[i];
            uint256 j = i;
            while (j != 0 && a[j - 1] > value) {
                a[j] = a[j - 1];
                --j;
            }
            a[j] = value;
        }
    }

    function sort(uint256[] memory a) internal pure {
        _sort(a, 0, a.length);
    }

    // Three-way partitioning bounds recursion on equal elements.
    function _sort(uint256[] memory a, uint256 lo, uint256 hi) private pure {
        while (hi - lo > 16) {
            uint256 pivot = a[lo + (hi - lo) / 2];
            uint256 left = lo;
            uint256 i = lo;
            uint256 right = hi;
            while (i < right) {
                if (a[i] < pivot) {
                    (a[left], a[i]) = (a[i], a[left]);
                    ++left;
                    ++i;
                } else if (a[i] > pivot) {
                    --right;
                    (a[i], a[right]) = (a[right], a[i]);
                } else {
                    ++i;
                }
            }
            if (left - lo < hi - right) {
                _sort(a, lo, left);
                lo = right;
            } else {
                _sort(a, right, hi);
                hi = left;
            }
        }
        for (uint256 i = lo + 1; i < hi; ++i) {
            uint256 value = a[i];
            uint256 j = i;
            while (j > lo && a[j - 1] > value) {
                a[j] = a[j - 1];
                --j;
            }
            a[j] = value;
        }
    }

    function reverse(uint256[] memory a) internal pure {
        for (uint256 i; i < a.length / 2; ++i) {
            uint256 j = a.length - 1 - i;
            (a[i], a[j]) = (a[j], a[i]);
        }
    }

    function copy(uint256[] memory a) internal pure returns (uint256[] memory result) {
        result = new uint256[](a.length);
        for (uint256 i; i < a.length; ++i) {
            result[i] = a[i];
        }
    }

    function isSorted(uint256[] memory a) internal pure returns (bool result) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] > a[i]) return false;
        }
        return true;
    }

    function isSortedAndUniquified(uint256[] memory a) internal pure returns (bool result) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] >= a[i]) return false;
        }
        return true;
    }

    function hasDuplicate(uint256[] memory a) internal pure returns (bool result) {
        if (a.length < 2) return false;
        uint256 capacity = 1;
        while (capacity < a.length * 2) capacity *= 2;
        uint256[] memory seen = new uint256[](capacity);
        uint256 mask = capacity - 1;
        for (uint256 i = a.length; i != 0;) {
            --i;
            // Use the upstream LibPRNG hash with checked array indexing.
            uint256 slot = mulmod(uint256(a[i]), 0x100000000000000000000000000000051, ~uint256(0xbc)) & mask;
            while (seen[slot] != 0) {
                if (a[seen[slot] - 1] == a[i]) return true;
                slot = (slot + 1) & mask;
            }
            seen[slot] = i + 1;
        }
        return false;
    }

    function insertionSort(int256[] memory a) internal pure {
        for (uint256 i = 1; i < a.length; ++i) {
            int256 value = a[i];
            uint256 j = i;
            while (j != 0 && a[j - 1] > value) {
                a[j] = a[j - 1];
                --j;
            }
            a[j] = value;
        }
    }

    function sort(int256[] memory a) internal pure {
        _sort(a, 0, a.length);
    }

    // Three-way partitioning bounds recursion on equal elements.
    function _sort(int256[] memory a, uint256 lo, uint256 hi) private pure {
        while (hi - lo > 16) {
            int256 pivot = a[lo + (hi - lo) / 2];
            uint256 left = lo;
            uint256 i = lo;
            uint256 right = hi;
            while (i < right) {
                if (a[i] < pivot) {
                    (a[left], a[i]) = (a[i], a[left]);
                    ++left;
                    ++i;
                } else if (a[i] > pivot) {
                    --right;
                    (a[i], a[right]) = (a[right], a[i]);
                } else {
                    ++i;
                }
            }
            if (left - lo < hi - right) {
                _sort(a, lo, left);
                lo = right;
            } else {
                _sort(a, right, hi);
                hi = left;
            }
        }
        for (uint256 i = lo + 1; i < hi; ++i) {
            int256 value = a[i];
            uint256 j = i;
            while (j > lo && a[j - 1] > value) {
                a[j] = a[j - 1];
                --j;
            }
            a[j] = value;
        }
    }

    function reverse(int256[] memory a) internal pure {
        for (uint256 i; i < a.length / 2; ++i) {
            uint256 j = a.length - 1 - i;
            (a[i], a[j]) = (a[j], a[i]);
        }
    }

    function copy(int256[] memory a) internal pure returns (int256[] memory result) {
        result = new int256[](a.length);
        for (uint256 i; i < a.length; ++i) {
            result[i] = a[i];
        }
    }

    function isSorted(int256[] memory a) internal pure returns (bool result) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] > a[i]) return false;
        }
        return true;
    }

    function isSortedAndUniquified(int256[] memory a) internal pure returns (bool result) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] >= a[i]) return false;
        }
        return true;
    }

    function hasDuplicate(int256[] memory a) internal pure returns (bool result) {
        if (a.length < 2) return false;
        uint256 capacity = 1;
        while (capacity < a.length * 2) capacity *= 2;
        uint256[] memory seen = new uint256[](capacity);
        uint256 mask = capacity - 1;
        for (uint256 i = a.length; i != 0;) {
            --i;
            // Use the upstream LibPRNG hash with checked array indexing.
            uint256 slot = mulmod(uint256(a[i]), 0x100000000000000000000000000000051, ~uint256(0xbc)) & mask;
            while (seen[slot] != 0) {
                if (a[seen[slot] - 1] == a[i]) return true;
                slot = (slot + 1) & mask;
            }
            seen[slot] = i + 1;
        }
        return false;
    }

    function insertionSort(address[] memory a) internal pure {
        for (uint256 i = 1; i < a.length; ++i) {
            address value = a[i];
            uint256 j = i;
            while (j != 0 && a[j - 1] > value) {
                a[j] = a[j - 1];
                --j;
            }
            a[j] = value;
        }
    }

    function sort(address[] memory a) internal pure {
        _sort(a, 0, a.length);
    }

    // Three-way partitioning bounds recursion on equal elements.
    function _sort(address[] memory a, uint256 lo, uint256 hi) private pure {
        while (hi - lo > 16) {
            address pivot = a[lo + (hi - lo) / 2];
            uint256 left = lo;
            uint256 i = lo;
            uint256 right = hi;
            while (i < right) {
                if (a[i] < pivot) {
                    (a[left], a[i]) = (a[i], a[left]);
                    ++left;
                    ++i;
                } else if (a[i] > pivot) {
                    --right;
                    (a[i], a[right]) = (a[right], a[i]);
                } else {
                    ++i;
                }
            }
            if (left - lo < hi - right) {
                _sort(a, lo, left);
                lo = right;
            } else {
                _sort(a, right, hi);
                hi = left;
            }
        }
        for (uint256 i = lo + 1; i < hi; ++i) {
            address value = a[i];
            uint256 j = i;
            while (j > lo && a[j - 1] > value) {
                a[j] = a[j - 1];
                --j;
            }
            a[j] = value;
        }
    }

    function reverse(address[] memory a) internal pure {
        for (uint256 i; i < a.length / 2; ++i) {
            uint256 j = a.length - 1 - i;
            (a[i], a[j]) = (a[j], a[i]);
        }
    }

    function copy(address[] memory a) internal pure returns (address[] memory result) {
        result = new address[](a.length);
        for (uint256 i; i < a.length; ++i) {
            result[i] = a[i];
        }
    }

    function isSorted(address[] memory a) internal pure returns (bool result) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] > a[i]) return false;
        }
        return true;
    }

    function isSortedAndUniquified(address[] memory a) internal pure returns (bool result) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] >= a[i]) return false;
        }
        return true;
    }

    function hasDuplicate(address[] memory a) internal pure returns (bool result) {
        if (a.length < 2) return false;
        uint256 capacity = 1;
        while (capacity < a.length * 2) capacity *= 2;
        uint256[] memory seen = new uint256[](capacity);
        uint256 mask = capacity - 1;
        for (uint256 i = a.length; i != 0;) {
            --i;
            // Use the upstream LibPRNG hash with checked array indexing.
            uint256 slot = mulmod(uint256(uint160(a[i])), 0x100000000000000000000000000000051, ~uint256(0xbc)) & mask;
            while (seen[slot] != 0) {
                if (a[seen[slot] - 1] == a[i]) return true;
                slot = (slot + 1) & mask;
            }
            seen[slot] = i + 1;
        }
        return false;
    }

    function insertionSort(bytes32[] memory a) internal pure {
        for (uint256 i = 1; i < a.length; ++i) {
            bytes32 value = a[i];
            uint256 j = i;
            while (j != 0 && a[j - 1] > value) {
                a[j] = a[j - 1];
                --j;
            }
            a[j] = value;
        }
    }

    function sort(bytes32[] memory a) internal pure {
        _sort(a, 0, a.length);
    }

    // Three-way partitioning bounds recursion on equal elements.
    function _sort(bytes32[] memory a, uint256 lo, uint256 hi) private pure {
        while (hi - lo > 16) {
            bytes32 pivot = a[lo + (hi - lo) / 2];
            uint256 left = lo;
            uint256 i = lo;
            uint256 right = hi;
            while (i < right) {
                if (a[i] < pivot) {
                    (a[left], a[i]) = (a[i], a[left]);
                    ++left;
                    ++i;
                } else if (a[i] > pivot) {
                    --right;
                    (a[i], a[right]) = (a[right], a[i]);
                } else {
                    ++i;
                }
            }
            if (left - lo < hi - right) {
                _sort(a, lo, left);
                lo = right;
            } else {
                _sort(a, right, hi);
                hi = left;
            }
        }
        for (uint256 i = lo + 1; i < hi; ++i) {
            bytes32 value = a[i];
            uint256 j = i;
            while (j > lo && a[j - 1] > value) {
                a[j] = a[j - 1];
                --j;
            }
            a[j] = value;
        }
    }

    function reverse(bytes32[] memory a) internal pure {
        for (uint256 i; i < a.length / 2; ++i) {
            uint256 j = a.length - 1 - i;
            (a[i], a[j]) = (a[j], a[i]);
        }
    }

    function copy(bytes32[] memory a) internal pure returns (bytes32[] memory result) {
        result = new bytes32[](a.length);
        for (uint256 i; i < a.length; ++i) {
            result[i] = a[i];
        }
    }

    function isSorted(bytes32[] memory a) internal pure returns (bool result) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] > a[i]) return false;
        }
        return true;
    }

    function isSortedAndUniquified(bytes32[] memory a) internal pure returns (bool result) {
        for (uint256 i = 1; i < a.length; ++i) {
            if (a[i - 1] >= a[i]) return false;
        }
        return true;
    }

    function hasDuplicate(bytes32[] memory a) internal pure returns (bool result) {
        if (a.length < 2) return false;
        uint256 capacity = 1;
        while (capacity < a.length * 2) capacity *= 2;
        uint256[] memory seen = new uint256[](capacity);
        uint256 mask = capacity - 1;
        for (uint256 i = a.length; i != 0;) {
            --i;
            // Use the upstream LibPRNG hash with checked array indexing.
            uint256 slot = mulmod(uint256(a[i]), 0x100000000000000000000000000000051, ~uint256(0xbc)) & mask;
            while (seen[slot] != 0) {
                if (a[seen[slot] - 1] == a[i]) return true;
                slot = (slot + 1) & mask;
            }
            seen[slot] = i + 1;
        }
        return false;
    }
}
