//@compile-flags: -Zcodegen --emit=bin
//@check-fail
// A member call on a receiver whose type is an unresolved import must produce
// diagnostics, not an ICE in codegen (the typeck-invariant panic).
import {Missing} from "./does-not-exist.sol"; //~ ERROR: file

contract MemberCallUnresolved {
    function f(Missing m) external { //~ ERROR: unresolved symbol `Missing`
        m.push(1); //~ ERROR: codegen does not support this `.push` member call yet
    }
}
