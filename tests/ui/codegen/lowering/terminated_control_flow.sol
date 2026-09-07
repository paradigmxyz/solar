//@ codegen-matrix: standard
//@ run-call: TerminatedControlFlow::constructorStop => 0
//@ run-call: TerminatedControlFlow::stopInHelper
//@ run-call: TerminatedControlFlow::trySuccess => 7
//@ run-call: TerminatedControlFlow::tryFailure => 9
//@ run-call: TerminatedControlFlow::breakSkipsTail => 0
//@ run-call: TerminatedControlFlow::continueSkipsTail => 0

contract TryTarget {
    function invoke(bool fail) external pure {
        if (fail) revert();
    }
}

contract TerminatedControlFlow {
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
