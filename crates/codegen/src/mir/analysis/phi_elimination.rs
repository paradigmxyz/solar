//! Phi elimination for MIR.
//!
//! Converts SSA phi nodes into parallel copies inserted at predecessor block exits.
//! This is necessary because the EVM cannot directly execute phi nodes.
//!
//! The algorithm:
//! 1. For each phi node in block B with incoming value V from predecessor P, insert a copy from V
//!    to the phi's destination at the end of P.
//! 2. Handle cycles by detecting when copies form a cycle and using a temporary.
//! 3. Remove the phi instructions after copies are inserted.
