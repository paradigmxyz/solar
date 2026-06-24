// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// The runtime differential suite installs this contract with `anvil_setCode`, so
// no constructor runs. Do not add constructor logic, `immutable`s, or state that
// must be initialized at deploy time: it would read as zero on both runtimes and
// hide real divergence. Storage starts empty and is mutated through the tx paths.
contract AbiVectorFixture {
    mapping(uint256 => uint256) private values;
    bytes private blob;

    // Emitted by the state-mutating paths so the differential suite also
    // compares LOG topics and data between solc and solar.
    event ValueSet(uint256 indexed key, uint256 value);
    event BlobSet(uint256 length, bytes32 digest);

    function f(
        uint256 value,
        bool flag,
        bytes memory data,
        string memory text
    ) external pure returns (uint256, bool, bytes32, bytes32, uint256, uint256) {
        return (
            value,
            flag,
            keccak256(data),
            keccak256(bytes(text)),
            data.length,
            bytes(text).length
        );
    }

    function numericFixed(
        int8 small,
        int256 signed,
        bytes1 one,
        bytes31 thirtyOne,
        bytes32 thirtyTwo,
        address account
    ) external pure returns (int8, int256, bytes1, bytes31, bytes32, address) {
        return (small, signed, one, thirtyOne, thirtyTwo, account);
    }

    function arrays(
        uint256[] calldata dynamicValues,
        uint256[3] calldata fixedValues
    ) external pure returns (uint256, uint256, uint256, uint256) {
        uint256 sum;
        for (uint256 i = 0; i < dynamicValues.length; ++i) {
            sum += dynamicValues[i];
        }
        return (
            dynamicValues.length,
            sum,
            fixedValues[0] + fixedValues[1] + fixedValues[2],
            fixedValues[2]
        );
    }

    function panicDiv(uint256 numerator, uint256 denominator) external pure returns (uint256) {
        return numerator / denominator;
    }

    function panicSub(uint256 lhs, uint256 rhs) external pure returns (uint256) {
        return lhs - rhs;
    }

    function arrayAt(uint256[] calldata values_, uint256 index) external pure returns (uint256) {
        return values_[index];
    }

    function setValue(uint256 key, uint256 value) external {
        values[key] = value;
        emit ValueSet(key, value);
    }

    function addValue(uint256 key, uint256 delta) external {
        values[key] += delta;
        emit ValueSet(key, values[key]);
    }

    function getValue(uint256 key) external view returns (uint256) {
        return values[key];
    }

    function setBlob(bytes calldata value) external {
        blob = value;
        emit BlobSet(value.length, keccak256(value));
    }

    function blobHash() external view returns (bytes32, uint256) {
        return (keccak256(blob), blob.length);
    }
}
