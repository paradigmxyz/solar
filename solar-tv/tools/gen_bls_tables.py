"""Copy EIP-2537's mathematical isogeny constants from ark-bls12-381 0.6.0.

Usage: uv run python solar-tv/tools/gen_bls_tables.py PATH/ark-bls12-381-0.6.0
This is a development-time data generator, not part of validation or replay.
"""

import argparse
import hashlib
import re
from pathlib import Path

P = int(
    "1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab",
    16,
)


def generate(source: Path) -> str:
    result = [
        "import SolarTV.EVMIR.Precompiles.Field\n",
        "/-! EIP-2537 isogeny constants, in ascending polynomial order. Generated",
        "by tools/gen_bls_tables.py from ark-bls12-381 0.6.0; only mathematical",
        "constants are copied, with negative values reduced to canonical residues.",
        "https://eips.ethereum.org/assets/eip-2537/field_to_curve -/\n",
        "namespace SolarTV.Precompiles.BLS12381.MapConstants\n",
    ]
    for group, sizes in [(1, [12, 11, 16, 16]), (2, [4, 3, 4, 4])]:
        path = source / f"src/curves/g{group}_swu_iso.rs"
        text = path.read_text()
        result.append(
            f"-- Source SHA-256: {hashlib.sha256(path.read_bytes()).hexdigest()}."
        )
        fields = [
            "x_map_numerator",
            "x_map_denominator",
            "y_map_numerator",
            "y_map_denominator",
        ]
        for field, size in zip(fields, sizes, strict=True):
            match = re.search(rf"{field}\s*:\s*&\[(.*?)\n\s*\]", text, re.DOTALL)
            if match is None:
                raise ValueError(f"missing {field}")
            numbers = [
                int(value) % P
                for value in re.findall(r'MontFp!\("(-?[0-9]+)"\)', match[1])
            ]
            if len(numbers) != size * group:
                raise ValueError(f"wrong number of {field} coefficients")
            values = (
                [str(value) for value in numbers]
                if group == 1
                else [
                    f"({a}, {b})"
                    for a, b in zip(numbers[::2], numbers[1::2], strict=True)
                ]
            )
            name = f"g{group}" + "".join(
                word.capitalize() for word in field.split("_")[::2]
            )
            kind = "Nat" if group == 1 else "F2"
            result.append(
                f"def {name} : List {kind} := [\n  " + ",\n  ".join(values) + "]\n"
            )
    result.append("end SolarTV.Precompiles.BLS12381.MapConstants\n")
    return "\n".join(result)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    args = parser.parse_args()
    target = (
        Path(__file__).resolve().parents[1]
        / "SolarTV/EVMIR/Precompiles/MapConstants.lean"
    )
    target.write_text(generate(args.source))
