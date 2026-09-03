contract Base { uint256 public x; constructor(uint256 v) { x = v; } }
// Deploy with v = 2**256 - 1: solc reverts with Panic(0x11), solar deploys and stores 0.
contract BaseArgAdd is Base { constructor(uint256 v) Base(v + 1) {} }
// Deploy with v = 2**255: solc reverts with Panic(0x11), solar deploys and stores 0.
contract BaseArgMul is Base { constructor(uint256 v) Base(v * 2) {} }
// The same arithmetic anywhere else traps on both compilers.
contract BaseArgCall is Base { constructor(uint256 v) Base(bump(v)) {} function bump(uint256 v) internal pure returns (uint256) { return v + 1; } }
contract BodyAdd is Base { constructor(uint256 v) Base(v) { x = v + 1; } }
contract ModArg { uint256 public x; modifier m(uint256 k) { x = k; _; } constructor(uint256 v) m(v + 1) {} }
