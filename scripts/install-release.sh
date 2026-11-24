# Appended to the generated release installer's configuration.
fail() {
  printf '%s\n' "$*" >&2
  exit 1
}

[[ "$(uname -s)/$(uname -m)" == Darwin/arm64 ]] || fail "This release requires an Apple Silicon Mac."
[[ $# -eq 0 ]] || fail "Run the installer without arguments."
destination="$HOME/Applications/$product_name.app"
[[ ! -e "$destination" && ! -L "$destination" ]] || fail "An app already exists at $destination. Use its in-app updater."
workspace="$(mktemp -d)"
trap 'rm -rf "$workspace"' EXIT
cd "$workspace"

for file in "$archive" "SHA256SUMS.txt"; do
  curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --output "$file" "$release_url/$file"
done
expected="$(awk -v file="$archive" '$2 == file {print $1}' "SHA256SUMS.txt")"
[[ "$expected" =~ ^[0-9a-f]{64}$ ]] || fail "The archive checksum is missing or invalid."
actual="$(shasum -a 256 "$archive" | cut -d ' ' -f 1)"
[[ "$actual" == "$expected" ]] || fail "The downloaded archive failed checksum verification."
ditto -x -k "$archive" unpacked
bundle="unpacked/$product_name.app"
codesign --verify --deep --strict -R "identifier \"$bundle_id\" and anchor apple generic" "$bundle"
spctl --assess --type execute "$bundle"
mkdir -p "$HOME/Applications"
mkdir "$destination" || fail "Could not reserve $destination."
if ! ditto "$bundle" "$destination"; then
  rm -rf "$destination"
  fail "Could not install $product_name."
fi
printf 'Installed %s\n' "$destination"
