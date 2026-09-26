#!/usr/bin/env bash
set -euo pipefail

revision=e0ea47a328e63ae2f35e1fc1d1b69c63d5505279
repo=https://github.com/planning-and-learning/tyr.git
install_root="$HOME/.local/share/tyr"
source_dir="$install_root/src"
build_dir="$install_root/build"
venv_dir="$install_root/venv"
binary="$build_dir/exe/gbfs_lazy"
link="$HOME/.local/bin/gbfs_lazy"

if [[ -e "$link" && ! -L "$link" ]] ||
   [[ -L "$link" && "$(readlink "$link")" != "$binary" ]]; then
    printf 'Refusing to replace unrelated executable or symlink: %s\n' "$link" >&2
    exit 1
fi

for tool in git cmake gcc g++ python3; do
    command -v "$tool" >/dev/null || { printf 'Required tool not found: %s\n' "$tool" >&2; exit 1; }
done
python3 -c 'import sys; sys.exit("Python 3.11 or newer is required") if sys.version_info < (3, 11) else None'

mkdir -p "$install_root"
if [[ ! -d "$source_dir/.git" ]]; then
    if [[ -e "$source_dir" ]]; then
        printf 'Refusing to replace non-Git Tyr source directory: %s\n' "$source_dir" >&2
        exit 1
    fi
    git clone --depth 1 "$repo" "$source_dir"
fi
if [[ ! -f "$source_dir/CMakeLists.txt" ]]; then
    tracked=$(git -C "$source_dir" ls-files | wc -l)
    deleted=$(git -C "$source_dir" ls-files -d | wc -l)
    committed=$(git -C "$source_dir" ls-tree -r --name-only HEAD | wc -l)
    untracked=$(git -C "$source_dir" ls-files --others --exclude-standard)
    if [[ -n "$untracked" ]] ||
       { [[ "$tracked" -gt 0 ]] && [[ "$deleted" -ne "$tracked" ]]; } ||
       [[ "$committed" -eq 0 ]]; then
        printf 'Refusing to update an incomplete Tyr checkout: %s\n' "$source_dir" >&2
        exit 1
    fi
elif [[ -n "$(git -C "$source_dir" status --porcelain)" ]]; then
    printf 'Refusing to update a modified Tyr checkout: %s\n' "$source_dir" >&2
    exit 1
fi
git -C "$source_dir" fetch --depth 1 origin "$revision"
git -C "$source_dir" -c advice.detachedHead=false checkout --detach "$revision"
[[ "$(git -C "$source_dir" rev-parse HEAD)" == "$revision" ]]

if [[ ! -x "$venv_dir/bin/python" ]]; then
    python3 -m venv "$venv_dir"
fi
"$venv_dir/bin/python" -m pip install --only-binary=:all: 'pyyggdrasil==0.2.5' 'pypddl==1.2.1'

prefixes=$("$venv_dir/bin/python" -c 'import pypddl, pyyggdrasil; print(f"{pypddl.native_prefix()};{pyyggdrasil.native_prefix()}")')
cmake -S "$source_dir" -B "$build_dir" \
    -DCMAKE_C_COMPILER=/usr/bin/gcc \
    -DCMAKE_CXX_COMPILER=/usr/bin/g++ \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_PREFIX_PATH="$prefixes" \
    -DPython_EXECUTABLE="$venv_dir/bin/python" \
    -DTYR_BUILD_EXECUTABLES=ON \
    -DTYR_ENABLE_LTO=OFF
cmake --build "$build_dir" --parallel 2 --target gbfs_lazy

mkdir -p "$(dirname "$link")"
if [[ -L "$link" ]]; then
    if [[ "$(readlink "$link")" != "$binary" ]]; then
        printf 'Refusing to replace a path changed during installation: %s\n' "$link" >&2
        exit 1
    fi
else
    if ! ln -s "$binary" "$link" &&
       [[ ! -L "$link" || "$(readlink "$link")" != "$binary" ]]; then
        printf 'Refusing to replace a path created during installation: %s\n' "$link" >&2
        exit 1
    fi
fi
printf 'Installed Tyr %s at %s\n' "$revision" "$link"
