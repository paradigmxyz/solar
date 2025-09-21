contract C {
    function a() public {
        assembly ("memory-safe") {
            let y := 9
        }

        assembly ("memory-safe", "memory-safe") { //~ ERROR: inline assembly marked memory-safe multiple times
            let y := 7
        }
        
        assembly ("unknown-flag") { //~ WARN: unknown inline assembly flag
            let y := 8
        }
    }
}
