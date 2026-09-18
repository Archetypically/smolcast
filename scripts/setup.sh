#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "$(uname -m)" != x86_64 ]] || ! command -v apt-get >/dev/null; then
  echo 'Supported Linux setup: x86-64 Debian/Ubuntu (including WSL).' >&2
  exit 1
fi

packages=(build-essential ca-certificates curl gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64)
missing=()
for package in "${packages[@]}"; do
  if [[ "$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null || true)" != 'install ok installed' ]]; then
    missing+=("$package")
  fi
done
if ((${#missing[@]})); then
  elevate=()
  if ((EUID != 0)); then elevate=(sudo); fi
  "${elevate[@]}" apt-get update
  "${elevate[@]}" apt-get install -y "${missing[@]}"
fi

# The repository pins the toolchain in rust-toolchain.toml; install that exact
# named toolchain rather than tracking stable.
TOOLCHAIN=1.98.1

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cargo_home=${CARGO_HOME:-"$HOME/.cargo"}
export PATH="$cargo_home/bin:$PATH"
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -fsS https://sh.rustup.rs -o "$work/rustup.sh"
  sh "$work/rustup.sh" -y --profile minimal --default-toolchain "$TOOLCHAIN"
fi
rustup toolchain install "$TOOLCHAIN" --profile minimal --component rustfmt --component clippy
rustup target add --toolchain "$TOOLCHAIN" x86_64-pc-windows-gnu

# Prove that the Windows import libraries and linker work, not just cargo check.
cat > "$work/probe.rs" <<'RS'
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
}
fn main() {
    println!("Windows process: {}", unsafe { GetCurrentProcessId() });
}
RS
rustc +"$TOOLCHAIN" --edition 2024 --target x86_64-pc-windows-gnu "$work/probe.rs" -o "$work/probe.exe"
echo 'Windows GNU toolchain smoke: linked a Windows API executable successfully.'
if [[ -f Cargo.toml ]]; then
  cargo +"$TOOLCHAIN" fetch --locked
else
  echo 'Dependencies ready. Application implementation is pending plan approval.'
fi
