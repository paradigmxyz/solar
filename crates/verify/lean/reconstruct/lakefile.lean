import Lake
open Lake DSL

-- cvc5 proof reconstruction for exported VCs, kept out of the base library so that building
-- SolarVerify needs no solver. See README.md.
require solar_verify from ".."
require smt from git "https://github.com/ufmg-smite/lean-smt.git" @ "6b26285ece45e38fc0d21809b944522233039829"

package solar_verify_reconstruct

@[default_target]
lean_lib SolarVerifyReconstruct
