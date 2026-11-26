#!/bin/sh
# Installs the `ahab` binary. It puts the binary in place
# and on your PATH: nothing runs until you run `ahab up`. It needs no root.
#
#   sh install.sh                                  the latest release from GitHub
#   AHAB_VERSION=v0.1.0-rc.1 sh install.sh         a given release (release candidates too)
#   sh install.sh path/to/ahab-linux-x86_64.tar.gz install an archive you have
#
# Every release has a .sha256 file next to each archive; the archive must
# match it.
#
#   AHAB_INSTALL_DIR      the folder for `ahab` (default ~/.local/bin)
#   AHAB_RELEASE_URL      where the archives are, instead of GitHub's release
#   AHAB_NO_MODIFY_PATH=1 leave your shell's startup file alone
set -eu

repo=https://github.com/spelvia/ahab
dir=${AHAB_INSTALL_DIR:-"$HOME/.local/bin"}

fail() {
	echo "Error: $*" >&2
	exit 1
}

case "$(uname -s)" in
Linux) os=linux ;;
Darwin) os=macos ;;
*) fail "Ahab runs on Linux and macOS, not $(uname -s)." ;;
esac
case "$(uname -m)" in
x86_64 | amd64) arch=x86_64 ;;
arm64 | aarch64) arch=aarch64 ;;
*) fail "no Ahab build for $(uname -m)." ;;
esac
name="ahab-$os-$arch.tar.gz"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if [ "$#" -gt 0 ]; then
	archive=$1
	[ -f "$archive" ] || fail "$archive does not exist."
	cp "$archive" "$work/$name"
	[ -f "$archive.sha256" ] && cp "$archive.sha256" "$work/$name.sha256"
else
	if [ -n "${AHAB_RELEASE_URL:-}" ]; then
		url=$AHAB_RELEASE_URL
	elif [ -n "${AHAB_VERSION:-}" ]; then
		url="$repo/releases/download/$AHAB_VERSION"
	else
		url="$repo/releases/latest/download"
	fi
	command -v curl >/dev/null || fail "curl is needed to download Ahab."
	echo "Downloading $name from $url…"
	curl --fail --location --silent --show-error -o "$work/$name" "$url/$name" ||
		fail "could not download $url/$name."
	curl --fail --location --silent --show-error -o "$work/$name.sha256" "$url/$name.sha256" ||
		fail "could not download $url/$name.sha256."
fi

[ -f "$work/$name.sha256" ] || fail "no $name.sha256 to check the archive against."
expected=$(cut -d' ' -f1 <"$work/$name.sha256")
if command -v sha256sum >/dev/null; then
	actual=$(sha256sum "$work/$name" | cut -d' ' -f1)
else
	actual=$(shasum -a 256 "$work/$name" | cut -d' ' -f1)
fi
[ "$expected" = "$actual" ] || fail "$name does not match its SHA-256; not installing it."

tar -xzf "$work/$name" -C "$work"
[ -f "$work/ahab" ] || fail "$name holds no ahab binary."
mkdir -p "$dir"
install -m 0755 "$work/ahab" "$dir/ahab"
echo "Installed $("$dir/ahab" --version) to $dir/ahab."

# Puts $dir on PATH for new shells: one line in the shell's startup file,
# added once.
on_path() {
	case ":$PATH:" in
	*":$dir:"*) return 0 ;;
	*) return 1 ;;
	esac
}
if ! on_path; then
	if [ "${AHAB_NO_MODIFY_PATH:-}" = 1 ]; then
		echo "Add $dir to your PATH to run it as \`ahab\`."
	else
		case "$(basename "${SHELL:-sh}")" in
		zsh)
			rc="${ZDOTDIR:-$HOME}/.zshrc"
			line="export PATH=\"$dir:\$PATH\""
			;;
		bash)
			rc="$HOME/.bashrc"
			line="export PATH=\"$dir:\$PATH\""
			;;
		fish)
			rc="${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/ahab.fish"
			line="fish_add_path \"$dir\""
			;;
		*)
			rc="$HOME/.profile"
			line="export PATH=\"$dir:\$PATH\""
			;;
		esac
		mkdir -p "$(dirname "$rc")"
		if [ -f "$rc" ] && grep -qF "$line" "$rc"; then
			:
		else
			printf '\n# Added by the Ahab installer\n%s\n' "$line" >>"$rc"
			echo "Added $dir to your PATH in $rc."
		fi
		echo "Open a new terminal, or run: export PATH=\"$dir:\$PATH\""
	fi
fi
echo "Next: run \`ahab up\` to start Ahab on this computer, or \`ahab setup\` to join it to a server."
