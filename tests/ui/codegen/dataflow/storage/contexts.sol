//@ revisions: summary object
//@[summary] compile-flags: -Zdataflow=storage
//@[object] compile-flags: -Zdataflow=storage -Zdataflow-k=1
//@[summary] filecheck: --check-prefix=SUMMARY
//@[object] filecheck: --check-prefix=OBJECT
// With `k = 0` the shared helper has one summary relative to its storage-pointer parameter,
// which each caller instantiates. With `k = 1` each call site analyzes the helper for the
// storage object it passes, the object-sensitive variant, and the summaries are concrete.

// SUMMARY: fn @bump:
// SUMMARY: summary: reads={arg0.1} writes={arg0.1}
// SUMMARY-NOT: fn @bump [
// SUMMARY: fn @first:
// SUMMARY: summary: reads={slot(1)} writes={slot(1)}
// SUMMARY: fn @second:
// SUMMARY: summary: reads={slot(2)[caller].1} writes={slot(2)[caller].1}

// OBJECT: fn @bump [@first](slot(0)):
// OBJECT: summary: reads={slot(1)} writes={slot(1)}
// OBJECT: fn @bump [@second](slot(2)[caller]):
// OBJECT: summary: reads={slot(2)[caller].1} writes={slot(2)[caller].1}
contract Contexts {
    struct Counter { uint owner; uint count; }
    Counter single;
    mapping(address => Counter) perUser;

    function bump(Counter storage counter) internal { counter.count += 1; }
    function first() external { bump(single); }
    function second() external { bump(perUser[msg.sender]); }
}
