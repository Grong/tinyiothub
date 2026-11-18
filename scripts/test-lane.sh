#!/usr/bin/env bash
set -euo pipefail

full_lane() {
    export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
    if [ -f scripts/tests/pre-push-fast.test.js ]; then
        "${NODE:-node}" --test scripts/tests/pre-push-fast.test.js
    fi
    cargo test --workspace --exclude zeroclaw --lib --bins --tests
    cd web && "${NPX:-npx}" vitest run
}

if [ "$#" -eq 0 ] || [ "$#" -gt 100 ]; then
    full_lane
    exit 0
fi

# Deletions have no executable path. Preserve the previous full-suite coverage
# instead of invoking a removed target or silently running nothing.
for file in "$@"; do
    case "$file" in
        Cargo.lock|Cargo.toml)
            full_lane
            exit 0
            ;;
    esac
    if [ ! -f "$file" ]; then
        full_lane
        exit 0
    fi
done

manifest_for() {
    local dir
    dir=$(dirname "$1")
    while [ "$dir" != "." ] && [ "$dir" != "/" ]; do
        if [ -f "$dir/Cargo.toml" ]; then
            printf '%s\n' "$dir/Cargo.toml"
            return 0
        fi
        dir=$(dirname "$dir")
    done
    return 1
}

integration_target() {
    local package_dir=$1 file=$2 relative first candidate
    relative=${file#"$package_dir"/tests/}
    if [ "$relative" = "${relative##*/}" ]; then
        basename "$relative" .rs
        return 0
    fi

    # A nested module belongs to the top-level target that declares its first
    # directory as a module (tests/e2e/foo.rs -> tests/e2e_tests.rs -> e2e_tests).
    first=${relative%%/*}
    for candidate in "$package_dir"/tests/*.rs; do
        [ -f "$candidate" ] || continue
        if grep -Eq "^[[:space:]]*(pub[[:space:]]+)?mod[[:space:]]+${first}[[:space:]]*;" "$candidate"; then
            basename "$candidate" .rs
            return 0
        fi
    done
    return 1
}

manifests=()
web_tests=()
playwright_tests=()
node_tests=()
full_web=false
for file in "$@"; do
    case "$file" in
        web/e2e/*.spec.ts)
            playwright_tests+=("${file#web/}")
            ;;
        web/*.test.ts|web/*.test.tsx|web/*.spec.ts|web/*.spec.tsx)
            web_tests+=("${file#web/}")
            ;;
        web/*.ts|web/*.tsx|web/package.json|web/package-lock.json)
            full_web=true
            ;;
        *.sh|*.bats)
            bash "$file"
            ;;
        *.test.js)
            node_tests+=("$file")
            ;;
        */Cargo.toml)
            manifests+=("$file")
            ;;
        *.rs)
            manifest=$(manifest_for "$file") || {
                echo "FAIL: no Cargo manifest for $file" >&2
                exit 1
            }
            already=false
            for existing in ${manifests[@]+"${manifests[@]}"}; do
                [ "$existing" = "$manifest" ] && already=true
            done
            $already || manifests+=("$manifest")
            ;;
        *)
            echo "FAIL: unsupported selected test file: $file" >&2
            exit 1
            ;;
    esac
done

for manifest in ${manifests[@]+"${manifests[@]}"}; do
    package_dir=$(dirname "$manifest")
    if [ "$package_dir" = "crates/driver-check" ]; then
        cargo build -p demo-declarative
    fi

    # Run only the selected integration target or source module. Cargo still
    # compiles the package, but does not execute every test in that package.
    ran_file=false
    for file in "$@"; do
        case "$file" in
            "$package_dir"/tests/*.rs)
                if target=$(integration_target "$package_dir" "$file"); then
                    cargo test --manifest-path "$manifest" --test "$target"
                else
                    cargo test --manifest-path "$manifest" --tests
                fi
                ran_file=true
                ;;
            "$package_dir"/src/*.rs)
                module=${file#"$package_dir"/src/}
                module=${module%.rs}
                module=${module%/mod}
                case "$module" in
                    lib)
                        cargo test --manifest-path "$manifest" --lib
                        ;;
                    main|bin/*)
                        cargo test --manifest-path "$manifest" --bins
                        ;;
                    *)
                        if [ -f "$package_dir/src/lib.rs" ]; then
                            cargo test --manifest-path "$manifest" --lib "${module//\//::}"
                        else
                            cargo test --manifest-path "$manifest" --bins "${module//\//::}"
                        fi
                        ;;
                esac
                ran_file=true
                ;;
        esac
    done
    $ran_file || cargo test --manifest-path "$manifest"
done

if [ "${#node_tests[@]}" -gt 0 ]; then
    export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
    "${NODE:-node}" --test "${node_tests[@]}"
fi

if $full_web; then
    export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
    (cd web && "${NPX:-npx}" vitest run)
elif [ "${#web_tests[@]}" -gt 0 ]; then
    export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
    (cd web && "${NPX:-npx}" vitest run "${web_tests[@]}")
fi

if [ "${#playwright_tests[@]}" -gt 0 ]; then
    export PATH="$HOME/.nvm/versions/node/v24.14.0/bin:$PATH"
    (cd web && "${NPX:-npx}" playwright test --reporter=list "${playwright_tests[@]}")
fi
