//@ codegen-matrix: standard
//@ run-call: noteThenAlloc 7, 9 => [7, 9]
//@ run-call: cleanAfterAlloc 0x1122334455 => 0xe04ba2b604e4f76f644d1d66c0fb9d706b7ff28769481902e8a5febfbf8dcb4d, 0
//@ run-call: 0x0617b06700000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000030000000000000000000000000000000000000000000000000000000000000041000000000000000000000000000000000000000000000000000000000000004200000000000000000000000000000000000000000000000000000000000000430000000000000000000000000000000000000000000000000000000000000043000000000000000000000000000000000000000000000000000000000000014d0000000000000000000000000000000000000000000000000000000000000041000000000000000000000000000000000000000000000000000000000000006f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 => 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000014d
//@ run-call: toByte 171 => 0xab
//@ run-call: toBytes 0x1234 => 0x0000000000000000000000000000000000000000000000000000000000001234
//@ run-call: zeroHash => 0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563
//@ run-call: selfMcall => 0, [0x, 0x]
//@ run-call: 0x4fa679d80000000000000000000000000000000000000000000000000000000000000077 => 0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000077

// Idioms from verified contracts that take fresh memory with `msize()`. Expected
// values come from solc 0.8.36 without optimization.
contract Idioms {
    // DSNote `note`: logs the call data from a buffer at `msize()`, and the body
    // allocates before it.
    modifier note() {
        _;
        assembly {
            let mark := msize()
            mstore(0x40, add(mark, 288))
            mstore(mark, 0x20)
            mstore(add(mark, 0x20), 224)
            calldatacopy(add(mark, 0x40), 0, 224)
            log4(mark, 288, shl(224, shr(224, calldataload(0))), caller(), calldataload(4), calldataload(36))
        }
    }

    function noteThenAlloc(uint256 a, uint256 b) external note returns (uint256[] memory out) {
        out = new uint256[](2);
        out[0] = a;
        out[1] = b;
    }

    // provable `safeMemoryCleaner`: zeroes everything from the free memory pointer
    // to `msize()`.
    function safeMemoryCleaner() internal pure {
        assembly {
            let fmem := mload(0x40)
            codecopy(fmem, codesize(), sub(msize(), fmem))
        }
    }

    function cleanAfterAlloc(bytes memory b) external pure returns (bytes32 h, uint256 tail) {
        bytes memory x = abi.encodePacked(b, b);
        safeMemoryCleaner();
        h = keccak256(x);
        assembly { tail := mload(mload(0x40)) }
    }

    // RedStone PriceAware `_readFromCallData`.
    function readFromCallData(bytes32[] memory symbols, uint256 dataSize, uint16 messageLength)
        external
        pure
        returns (uint256[] memory)
    {
        uint256[] memory values;
        uint256 i;
        uint256 j;
        uint256 readyAssets;
        bytes32 currentSymbol;
        assembly {
            let start := sub(calldatasize(), add(messageLength, 66))
            values := msize()
            mstore(values, mload(symbols))
            mstore(0x40, add(add(values, 0x20), mul(mload(symbols), 0x20)))
            for { i := 0 } lt(i, dataSize) { i := add(i, 1) } {
                currentSymbol := calldataload(add(start, mul(i, 64)))
                for { j := 0 } lt(j, mload(symbols)) { j := add(j, 1) } {
                    if eq(mload(add(add(symbols, 32), mul(j, 32))), currentSymbol) {
                        mstore(add(add(values, 32), mul(j, 32)), calldataload(add(add(start, mul(i, 64)), 32)))
                        readyAssets := add(readyAssets, 1)
                    }
                    if eq(readyAssets, mload(symbols)) { i := dataSize }
                }
            }
        }
        return (values);
    }

    // Integers `toByte` and `toBytes`: write one byte past `msize()`.
    function toByte(uint8 _base) external pure returns (bytes1 _ret) {
        assembly {
            let m_alloc := add(msize(), 0x1)
            mstore8(m_alloc, _base)
            _ret := mload(m_alloc)
        }
    }

    function toBytes(uint256 _base) external pure returns (bytes memory _ret) {
        assembly {
            let m_alloc := add(msize(), 0x1)
            _ret := mload(m_alloc)
            mstore(_ret, 0x20)
            mstore(add(_ret, 0x20), _base)
        }
    }

    // The word at `msize()` is zero.
    function zeroHash() external pure returns (bytes32 h) {
        assembly { h := keccak256(add(msize(), 0), 0x20) }
    }

    // Storage `mcall`: collects the return data of each call at `msize()`.
    struct Call {
        address target;
        bytes callData;
    }

    function mcall(Call[] memory calls) public returns (uint256 blockNumber, bytes[] memory returnData) {
        assembly {
            blockNumber := number()
            returnData := msize()
            mstore(returnData, mload(calls))
            let retDataOffset := add(returnData, 0x20)
            for { let i := 0 } lt(i, mload(calls)) { i := add(i, 1) } {
                let callItem := mload(add(calls, add(0x20, mul(i, 0x20))))
                let target := mload(callItem)
                let callData := mload(add(callItem, 0x20))
                let callDataLen := mload(callData)
                let success := call(gas(), target, 0, add(callData, 0x20), callDataLen, 0, 0)
                if iszero(success) { revert(0, 0) }
                let retLen := returndatasize()
                mstore(retDataOffset, retLen)
                returndatacopy(add(retDataOffset, 0x20), 0, retLen)
                retDataOffset := add(retDataOffset, add(0x20, retLen))
            }
            mstore(0x40, retDataOffset)
        }
    }

    function selfMcall() external returns (uint256, bytes[] memory) {
        Call[] memory calls = new Call[](2);
        calls[0] = Call(address(this), abi.encodeCall(this.toBytes, (0xabcdef)));
        calls[1] = Call(address(this), abi.encodeCall(this.zeroHash, ()));
        (, bytes[] memory r) = mcall(calls);
        return (0, r);
    }

    // Optherium `_returnReturnData`: copies the return data to `msize()` and returns it.
    function _returnReturnData(bool _success) internal pure {
        assembly {
            let returndatastart := msize()
            mstore(0x40, add(returndatastart, returndatasize()))
            returndatacopy(returndatastart, 0, returndatasize())
            switch _success case 0 { revert(returndatastart, returndatasize()) } default { return(returndatastart, returndatasize()) }
        }
    }

    function forward(uint256 x) external {
        (bool ok,) = address(this).call(abi.encodeCall(this.toBytes, (x)));
        _returnReturnData(ok);
    }
}
