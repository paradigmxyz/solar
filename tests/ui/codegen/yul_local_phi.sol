//@ignore-host: windows
//@compile-flags: -Zcodegen --emit=mir

contract YulLocalPhi {
    function branchLocal(uint256 flag) public pure returns (uint256 result) {
        assembly {
            let x := 1
            if flag {
                x := 2
            }
            result := x
        }
    }
}
