contract C {
    function a() public {
        assembly ("memory-safe", "memory-safe") { //~ ERROR: Inline assembly marked memory-safe multiple times
            let y := 7
        }
        
        // TODO: Add warning for unknown flags
        // assembly ("unknown-flag") {
        //     let y := 8
        // }
        
        assembly ("memory-safe") {
            let y := 9
        }
    }
}