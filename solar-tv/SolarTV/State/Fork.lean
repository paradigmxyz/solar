/-! Fork ordering shared by state and gas semantics. The constructors match the
compiler's EvmVersion schema. Feature availability and gas are separate: an
unavailable opcode is exceptional, not an expensive version of a legal one. -/

namespace SolarTV

inductive Fork where
  | homestead | tangerineWhistle | spuriousDragon | byzantium
  | constantinople | petersburg | istanbul | berlin | london | paris
  | shanghai | cancun | prague | osaka | amsterdam
  deriving DecidableEq, Repr

def Fork.ordinal : Fork → Nat
  | .homestead => 0 | .tangerineWhistle => 1 | .spuriousDragon => 2
  | .byzantium => 3 | .constantinople => 4 | .petersburg => 5
  | .istanbul => 6 | .berlin => 7 | .london => 8 | .paris => 9
  | .shanghai => 10 | .cancun => 11 | .prague => 12 | .osaka => 13
  | .amsterdam => 14

def Fork.atLeast (fork since : Fork) : Bool := since.ordinal ≤ fork.ordinal

end SolarTV
