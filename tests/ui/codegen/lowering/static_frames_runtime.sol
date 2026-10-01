//@ codegen-matrix: standard
//@ run-call: StaticFramesRuntime::top 0 => 16
//@ run-call: StaticFramesRuntime::top 1 => 45
//@ run-call: StaticFramesRuntime::top 4 => 1170

import "./static_frames.sol";

// Execute the same static/recursive/mutually-recursive frame graph whose layout
// is asserted by static_frames.sol, in every optimization mode.
contract StaticFramesRuntime is SF {}
