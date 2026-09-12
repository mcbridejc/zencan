#!/usr/bin/env bash
set -euo pipefail

locked=()
if [[ ${1:-} == "--locked" ]]; then
    locked=(--locked)
elif [[ $# -ne 0 ]]; then
    echo "usage: $0 [--locked]" >&2
    exit 2
fi

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
rows=()
failures=0

measure() {
    local name=$1
    local target=$2
    local features=${3:-}
    local directory="$root/examples/$name"
    local output values text data bss sections flash ram

    local feature_args=()
    if [[ -n $features ]]; then
        feature_args=(--features "$features")
    fi

    if ! output=$(cd "$directory" && cargo size --release --bin "$name" --target "$target" "${feature_args[@]}" "${locked[@]}" 2>&1); then
        printf '%s\n' "$output" >&2
        rows+=("$name|—|—|—|—|—|—")
        failures=$((failures + 1))
        return 0
    fi
    values=$(printf '%s\n' "$output" | awk '$1 ~ /^[0-9]+$/ && $2 ~ /^[0-9]+$/ && $3 ~ /^[0-9]+$/ && $4 ~ /^[0-9]+$/ { print $1, $2, $3, $4 }' | tail -n 1)
    if [[ -z $values ]]; then
        printf '%s\n' "$output" >&2
        echo "cargo size did not produce a Berkeley-format size row for $name" >&2
        rows+=("$name|—|—|—|—|—|—")
        failures=$((failures + 1))
        return 0
    fi

    read -r text data bss sections <<< "$values"
    flash=$((text + data))
    ram=$((data + bss))
    rows+=("$name|$flash|$ram|$text|$data|$bss|$sections")
}

measure stm32g0-lilos-node thumbv6m-none-eabi
measure esp-node riscv32imc-unknown-none-elf esp32c3

printf '# Example binary sizes\n\n'
printf '| %-21s | %18s | %16s | %10s | %10s | %10s | %25s |\n' \
    'Example' 'Flash (text + data)' 'RAM (data + bss)' 'Text' 'Data' 'BSS' 'Sections (text + data + bss)'
printf '| %-21s | %18s: | %16s: | %10s: | %10s: | %10s: | %25s: |\n' \
    '---' '---' '---' '---' '---' '---' '---'
for row in "${rows[@]}"; do
    IFS='|' read -r name flash ram text data bss sections <<< "$row"
    printf '| %-21s | %18s | %16s | %10s | %10s | %10s | %25s |\n' \
        "$name" "$flash" "$ram" "$text" "$data" "$bss" "$sections"
done
cat <<'EOF'

`cargo size` reports allocated text, data, and BSS sections. Flash is text plus initialized data; RAM is initialized data plus BSS. Sections is the total reported by `cargo size`.
EOF

exit "$failures"
