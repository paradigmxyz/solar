//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 4, 32, 7 => 3004
//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 67, 32, 7 => 3067
//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 74, 32, 7 => 3074
//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 75, 32, 7 => 1007
//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 256, 32, 7 => 1007
//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 68, 32, 0 => 1000
//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 68, 0, 7 => 1000
//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 68, 64, 0 => 3068
//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 68, 0x10000000000000000, 0 => 3068
//@ run-call: TryCatchMalformed::directError(uint256,uint256,uint256) 68, 32, 0x10000000000000000 => 3068
//@ run-call: TryCatchMalformed::directPanic(uint256) 35 => 3035
//@ run-call: TryCatchMalformed::directPanic(uint256) 36 => 2067
//@ run-call: TryCatchMalformed::withoutBinding() => 3
//@ run-call-fail: TryCatchMalformed::withoutGeneric() => 0x08c379a0
//@ run-call: TryCatchMalformed::creation() => 3004
//@ run-call: TryCatchMalformed::functionPointer() => 3004
//@ run-call: TryCatchMalformed::customError() => 3004

contract MalformedTryTarget {
    constructor(uint256 size, uint256 offset, uint256 length) {
        if (size != 0) _error(size, offset, length);
    }

    function failError(uint256 size, uint256 offset, uint256 length) external pure {
        _error(size, offset, length);
    }

    function failPanic(uint256 size) external pure {
        assembly ("memory-safe") {
            mstore(0, shl(224, 0x4e487b71))
            mstore(4, 0x43)
            revert(0, size)
        }
    }

    function failCustom() external pure {
        assembly ("memory-safe") {
            mstore(0, shl(224, 0xdeadbeef))
            revert(0, 4)
        }
    }

    function _error(uint256 size, uint256 offset, uint256 length) private pure {
        assembly ("memory-safe") {
            mstore(0, shl(224, 0x08c379a0))
            mstore(4, offset)
            mstore(36, length)
            mstore(68, "abcdefg")
            revert(0, size)
        }
    }
}

contract TryCatchMalformed {
    function directError(uint256 size, uint256 offset, uint256 length)
        external
        returns (uint256)
    {
        MalformedTryTarget target = new MalformedTryTarget(0, 0, 0);
        try target.failError(size, offset, length) {} catch Error(string memory reason) {
            return 1000 + bytes(reason).length;
        } catch Panic(uint256 code) {
            return 2000 + code;
        } catch (bytes memory data) {
            return 3000 + data.length;
        }
        return 0;
    }

    function directPanic(uint256 size) external returns (uint256) {
        MalformedTryTarget target = new MalformedTryTarget(0, 0, 0);
        try target.failPanic(size) {} catch Error(string memory reason) {
            return 1000 + bytes(reason).length;
        } catch Panic(uint256 code) {
            return 2000 + code;
        } catch (bytes memory data) {
            return 3000 + data.length;
        }
        return 0;
    }

    function withoutBinding() external returns (uint256) {
        MalformedTryTarget target = new MalformedTryTarget(0, 0, 0);
        try target.failError(4, 32, 7) {} catch Error(string memory) {
            return 1;
        } catch Panic(uint256) {
            return 2;
        } catch {
            return 3;
        }
        return 0;
    }

    function withoutGeneric() external returns (uint256) {
        MalformedTryTarget target = new MalformedTryTarget(0, 0, 0);
        try target.failError(4, 32, 7) {} catch Error(string memory) {
            return 1;
        } catch Panic(uint256) {
            return 2;
        }
        return 0;
    }

    function creation() external returns (uint256) {
        try new MalformedTryTarget(4, 32, 7) returns (MalformedTryTarget) {
            return 0;
        } catch Error(string memory) {
            return 1;
        } catch Panic(uint256) {
            return 2;
        } catch (bytes memory data) {
            return 3000 + data.length;
        }
    }

    function functionPointer() external returns (uint256) {
        MalformedTryTarget target = new MalformedTryTarget(0, 0, 0);
        function(uint256, uint256, uint256) external pure pointer = target.failError;
        try pointer(4, 32, 7) {} catch Error(string memory) {
            return 1;
        } catch Panic(uint256) {
            return 2;
        } catch (bytes memory data) {
            return 3000 + data.length;
        }
        return 0;
    }

    function customError() external returns (uint256) {
        MalformedTryTarget target = new MalformedTryTarget(0, 0, 0);
        try target.failCustom() {} catch Error(string memory) {
            return 1;
        } catch Panic(uint256) {
            return 2;
        } catch (bytes memory data) {
            return 3000 + data.length;
        }
        return 0;
    }
}
