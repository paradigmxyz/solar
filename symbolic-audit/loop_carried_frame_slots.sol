// Finding 32: loop-carried values are kept in memory frame slots, so tight loops cost 1.25x to 1.7x solc.
//   python3 symbolic-audit/tools/statediff.py symbolic-audit/loop_carried_frame_slots.sol Y --gas 500000000 --keep \
//     --fixed "yulFunction(uint256) 100000" --fixed "yulTopLevel(uint256) 100000" --fixed "solidityLoop(uint256) 100000" --fixed "solidityCall(uint256) 100000"
//   then read out/gas.txt in the kept project.
contract Y {
    function yulFunction(uint256 a) external pure returns (uint256 b) {
        assembly {
            function fac(n) -> nf { nf := 1 for { let i := n } gt(i, 0) { i := sub(i, 1) } { nf := mul(nf, i) } }
            b := fac(a)
        }
    }
    function yulTopLevel(uint256 a) external pure returns (uint256 b) {
        assembly { b := 1 for { let i := a } gt(i, 0) { i := sub(i, 1) } { b := mul(b, i) } }
    }
    function solidityLoop(uint256 a) external pure returns (uint256 b) {
        b = 1;
        unchecked { for (uint256 i = a; i > 0; i--) { b *= i; } }
    }
    function solidityCall(uint256 a) external pure returns (uint256) { return fac(a); }
    function fac(uint256 n) internal pure returns (uint256 nf) {
        nf = 1;
        unchecked { for (uint256 i = n; i > 0; i--) { nf *= i; } }
    }
    function yulFunctionNoLoop(uint256 a) external pure returns (uint256 b) {
        assembly {
            function sq(x) -> r { r := mul(x, x) }
            b := sq(a)
        }
    }
}
