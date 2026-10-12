// solc rejects these Yul declarations with errors 3859, 4113 and 5017; see TYPECK-012 in
// docs/SOLC_DIVERGENCE.md.

contract C {
    uint s;

    function illegalNames() public pure {
        assembly {
            function this() {}
            function super() {}
            function _() {}
            function a(this, super, _) {}
            function b() -> this, super, _ {}
        }
        assembly {
            let this := 1
            let super := 1
            let _ := 1
        }
    }

    function reservedIdentifiers() public pure {
        assembly {
            function dup1(dup2) -> dup3 {}
            function loadimmutable(setimmutable) -> datasize {}
            function linkersymbol(x) {}
            linkersymbol("contract/library.sol:L")
        }
        assembly {
            let dataoffset := 1
            let datacopy := 1
            let jump := 1
            let jumpi := 1
            let jumpdest := 1
            let pc := 1
            let push0 := 1
            let push32 := 1
            let dup16 := 1
            let swap16 := 1
        }
    }

    function shadowing(uint a) public pure returns (uint r) {
        uint b;
        assembly {
            let a := 1
            let b := 1
            let C := 1
            let shadowing := 1
            let s := 1
            let msg := 1
            function g() -> x {
                let r := 1
            }
        }
    }
}
