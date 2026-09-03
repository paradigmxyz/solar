// Finding 33: `this.f()` inside an internal function that the constructor calls skips the
// extcodesize guard, so the call to the not-yet-deployed contract silently does nothing where
// solc reverts the deployment.
//   python3 symbolic-audit/tools/statediff.py symbolic-audit/this_call_from_constructor_helper.sol R --calls 2 --seqs 1
//   python3 symbolic-audit/tools/statediff.py symbolic-audit/this_call_from_constructor_helper.sol Direct --calls 2 --seqs 1
contract R {
    uint256 public seen;
    constructor() { helper(); }
    function helper() internal { this.setIt(); }
    function setIt() external { seen = seen + 1; }
    function direct() external { this.setIt(); }
}
contract Direct {
    uint256 public seen;
    constructor() { this.setIt(); }
    function setIt() external { seen = seen + 1; }
}
contract ViaModifier {
    uint256 public seen;
    modifier m() { this.setIt(); _; }
    constructor() m() {}
    function setIt() external { seen = seen + 1; }
}
contract ViaBase {
    uint256 public seen;
    constructor() { this.setIt(); }
    function setIt() external { seen = seen + 1; }
}
contract Derived is ViaBase { constructor() ViaBase() {} }
contract Runtime {
    uint256 public seen;
    function helper() internal { this.setIt(); }
    function viaHelper() external { helper(); }
    function setIt() external { seen = seen + 1; }
}
