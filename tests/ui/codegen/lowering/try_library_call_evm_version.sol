//@ revisions: homestead byzantium
//@[homestead] compile-flags: -O none --evm-version homestead -Zdump=mir
//@[homestead] filecheck: --check-prefix=HOMESTEAD --implicit-check-not=returndatasize
//@[byzantium] compile-flags: -O none --evm-version byzantium -Zdump=mir
//@[byzantium] filecheck: --check-prefix=BYZANTIUM

library TryLib {
    function double(uint256 x) external pure returns (uint256) {
        return 2 * x;
    }

    function noop(uint256) external pure {}
}

contract TryLibraryCall {
    uint256 public seen;

    // solc compiles a `try` around an external library call before Byzantium as well: the
    // delegatecall's static output size carries the return values, with no return data
    // involved. A library call outside a `try` still needs Byzantium here.
    // HOMESTEAD-LABEL: fn @libCall
    // HOMESTEAD: extcodesize
    // HOMESTEAD: [[GAS:v[0-9]+]] = gas
    // HOMESTEAD: [[FWD:v[0-9]+]] = sub [[GAS]], 50
    // HOMESTEAD: delegatecall [[FWD]], {{.*}}, 0, 32
    // BYZANTIUM-LABEL: fn @libCall
    // BYZANTIUM: delegatecall {{.*}}, 0, 0
    // BYZANTIUM: returndatasize
    function libCall(uint256 x) external returns (uint256 r) {
        try TryLib.double(x) returns (uint256 v) {
            seen = v;
            r = v;
        } catch {
            r = 7;
        }
    }

    // HOMESTEAD-LABEL: fn @libNoReturn
    // HOMESTEAD: extcodesize
    // HOMESTEAD: delegatecall {{.*}}, 0, 0
    function libNoReturn(uint256 x) external returns (uint256 r) {
        try TryLib.noop(x) {
            seen = 1;
            r = 1;
        } catch {
            r = 7;
        }
    }
}
