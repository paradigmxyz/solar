//@ codegen-matrix: standard
//@ run-call: TerminatedControlFlow::constructorStop => 0
//@ run-call: TerminatedControlFlow::stopInHelper
//@ run-call: TerminatedControlFlow::trySuccess => 7
//@ run-call: TerminatedControlFlow::tryFailure => 9
//@ run-call: TerminatedControlFlow::breakSkipsTail => 0
//@ run-call: TerminatedControlFlow::continueSkipsTail => 0

//@ run-call-fail: TerminatedControlFlow::scalarFailure 17 => 0x0000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: TerminatedControlFlow::pairFailure 19 => 0x0000000000000000000000000000000000000000000000000000000000000013
//@ run-call-fail: TerminatedControlFlow::recursiveFailure 3, 23 => 0x0000000000000000000000000000000000000000000000000000000000000017
//@ run-call-fail: TerminatedControlFlow::constructorFailure 29 => 0x000000000000000000000000000000000000000000000000000000000000001d
//@ run-call-fail: TerminatedControlFlow::oversizedReturn => 0x
//@ run-call-fail: TerminatedControlFlow::unaffordableReturn => 0x
//@ run-call: TerminatedControlFlow::precompileReturndata 2, 0 => 96
//@ run-call: TerminatedControlFlow::precompileReturndata 2, 1 => 96
//@ run-call: TerminatedControlFlow::precompileReturndata 2, 2 => 96
//@ run-call: TerminatedControlFlow::storageRevertPayload => true
//@ run-call: TerminatedControlFlow::maybeFailure false => 7
//@ run-call: TerminatedControlFlow::constructorWithoutArgs => true
//@ run-call-fail: TerminatedControlFlow::maybeFailure true => 0x

contract TryTarget {
    function invoke(bool fail) external pure {
        if (fail) revert();
    }
}

contract TerminatedControlFlow {
    bytes private revertData;
    error StorageFailure(bytes data);
    TryTarget private target;

    constructor() {
        target = new TryTarget();
    }

    function trySuccess() external view returns (uint256) {
        try target.invoke(false) {
            return 7;
        } catch {
            return 9;
        }
    }

    function tryFailure() external view returns (uint256) {
        try target.invoke(true) {
            return 7;
        } catch {
            return 9;
        }
    }

    function constructorStop() external returns (uint256) {
        return address(new StopsInConstructor()).code.length;
    }

    function stopInHelper() external pure {
        stopHelper();
        revert();
    }

    function stopHelper() internal pure {
        assembly { stop() }
    }

    function scalarFailure(uint256 reason) external pure returns (uint256) {
        return failScalar(reason) + 1;
    }

    function failScalar(uint256 reason) internal pure returns (uint256) {
        assembly { mstore(0, reason) revert(0, 32) }
    }

    function pairFailure(uint256 reason) external pure returns (uint256, uint256) {
        (uint256 a, uint256 b) = failPair(reason);
        return (a + 1, b + 2);
    }

    function failPair(uint256 reason) internal pure returns (uint256, uint256) {
        assembly { mstore(0, reason) revert(0, 32) }
    }

    function recursiveFailure(uint256 depth, uint256 reason) public pure returns (uint256) {
        if (depth == 0) {
            assembly { mstore(0, reason) revert(0, 32) }
        }
        return recursiveFailure(depth - 1, reason) + 1;
    }

    function constructorFailure(uint256 reason) external returns (address) {
        return address(new RevertsInConstructor(reason));
    }

    function constructorWithoutArgs() external returns (bool) {
        try new RevertsWithoutArgs() {
            return false;
        } catch (bytes memory reason) {
            return keccak256(reason) == keccak256(abi.encode(
                block.number + 11, block.timestamp + 22, block.chainid + 33
            ));
        }
    }

    function oversizedReturn() external pure {
        assembly {
            mstore(0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0, 7)
            return(0x123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0, 32)
        }
    }

    function unaffordableReturn() external pure {
        assembly {
            mstore(0x123456789abcdef0, 7)
            return(0x123456789abcdef0, 32)
        }
    }

    function precompileReturndata(uint256 rounds, uint256 kind) external pure returns (uint256 total) {
        uint256 i;
        do {
            assembly { total := add(total, mul(returndatasize(), 3)) }
            if (kind == 0) {
                sha256(abi.encode(i));
            } else if (kind == 1) {
                ripemd160(abi.encode(i));
            } else {
                ecrecover(
                    bytes32(uint256(1)),
                    28,
                    0x6673ffad2147741f04772b6f921f0ba6af0c1e77fc439e65c36dedf4092e8898,
                    0x4c1a971652e0ada880120ef8025e709fff2080c4a39aae068d12eed009b68c89
                );
            }
            ++i;
        } while (i < rounds);
    }

    function storageRevertPayload() external returns (bool) {
        try this.failWithStorageBytes(3) {
            return false;
        } catch (bytes memory reason) {
            return keccak256(reason) == keccak256(abi.encodeWithSelector(StorageFailure.selector, hex"00000000"));
        }
    }

    function failWithStorageBytes(uint256 rounds) external {
        assembly { sstore(revertData.slot, 2) }
        for (uint256 i; i < rounds; ++i) {
            assembly { sstore(revertData.slot, add(sload(revertData.slot), 2)) }
            if (i == 2) revert StorageFailure(revertData);
        }
    }

    function maybeFailure(bool fail) external pure returns (uint256) {
        return maybeFail(fail) + 1;
    }

    function maybeFail(bool fail) internal pure returns (uint256) {
        if (fail) revert();
        return 6;
    }

    function breakSkipsTail() external pure returns (uint256 result) {
        for (uint256 i = 0; i < 1; ++i) {
            break;
            result = 1;
        }
    }

    function continueSkipsTail() external pure returns (uint256 result) {
        for (uint256 i = 0; i < 1; ++i) {
            continue;
            result = 1;
        }
    }
}

contract StopsInConstructor {
    constructor() {
        assembly { stop() }
    }

    function present() external pure returns (uint256) {
        return 1;
    }
}

contract RevertsInConstructor {
    constructor(uint256 reason) {
        uint256 result = fail(reason);
        assembly { sstore(0, result) }
    }

    function fail(uint256 reason) internal pure returns (uint256) {
        assembly { mstore(0, reason) revert(0, 32) }
    }
}

contract RevertsWithoutArgs {
    constructor() {
        uint256 result = fail();
        assembly { sstore(0, result) }
    }

    function fail() internal view returns (uint256) {
        uint256 a = block.number + 11;
        uint256 b = block.timestamp + 22;
        uint256 c = block.chainid + 33;
        if (gasleft() > 0) {
            assembly {
                mstore(0, a)
                mstore(32, b)
                mstore(64, c)
                revert(0, 96)
            }
        }
        revert();
    }
}
