//@ codegen-matrix: standard
//@ run-call: constructorAndRuntime => true

contract LiteralArtifacts {
    bytes32 private immutable initial;

    constructor() {
        initial = keccak256(bytes(version()));
    }

    function version() public pure returns (string memory) {
        return "1";
    }

    function constructorAndRuntime() external view returns (bool) {
        return initial == keccak256(bytes(this.version()));
    }

}
