# Ubuntu 24.04 package

Native amd64 build of [mstoeckl/swaylock-plugin](https://github.com/mstoeckl/swaylock-plugin),
version 1.8.7 at commit `30127fb0376a13cd45658ad8900e2b9f9f116cf1`.
It installs `/usr/bin/swaylock-plugin`, `/usr/bin/swaylock-sleep-watcher`,
the manual, completions, and `/etc/pam.d/swaylock-plugin`. It does not replace
Ubuntu's swaylock. PAM uses `auth include login`, matching Ubuntu's swaylock
configuration and retaining the host's SSSD authentication.

[Download the `.deb`, buildable source archive, and checksums](https://github.com/madprogrammer/verdant/releases/tag/swaylock-plugin-1.8.7-noble).

## Build

Use an Ubuntu 24.04 build environment with:

```sh
sudo apt-get install build-essential debhelper meson ninja-build pkg-config \
  libwayland-dev libxkbcommon-dev libcairo2-dev libpam0g-dev libsystemd-dev scdoc
```

The source preparation script takes a checkout of the pinned revision and the
`share/wayland-protocols` directory from Wayland Protocols **1.49**. This version
is available from Verdant's pinned Nixpkgs. The XML is only used during the build;
the resulting package uses Ubuntu's native Wayland libraries and does not need Nix.
Only the eight required XML files are copied, with their copyright notices.

```sh
env -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY \
  -u http_proxy -u https_proxy -u all_proxy \
  proxychains4 git clone https://github.com/mstoeckl/swaylock-plugin.git upstream
git -C upstream checkout 30127fb0376a13cd45658ad8900e2b9f9f116cf1
protocols=$(nix build --inputs-from . nixpkgs#wayland-protocols --no-link --print-out-paths)
packaging/swaylock-plugin/prepare-source.sh upstream \
  "$protocols/share/wayland-protocols" swaylock-plugin-build
```

Copy the prepared directory to the Ubuntu build environment, then:

```sh
cd swaylock-plugin-build
dpkg-buildpackage -b -us -uc
```

## Install and use

```sh
sudo apt install ./swaylock-plugin_1.8.7+git20261028.30127fb-1~ubuntu24.04.1_amd64.deb
swaylock-plugin -f -c 020805 \
  --command-each 'exec verdant --fps 12 --max-width 1280 --columns 200 --brightness 0.75'
```

`--command-each` starts one Verdant process per locked output. Each receives an
inherited Wayland socket from the locker; no output names or separate desktop
wallpaper supervisor are needed. GPU buffers and frame callbacks pass through
the locker. There is no grace period in this invocation. The `-f` readiness
handshake is suitable for swayidle's `before-sleep` hook.

The package is built against Ubuntu libraries, including PAM, with Debian's
hardening flags. Do not replace it with a Nix-linked locker on hosts that depend
on Ubuntu PAM modules. Verdant itself can remain installed through Nix.

## Validation

Built with `dpkg-buildpackage` in an isolated Ubuntu 24.04 amd64 filesystem.
On the Ubuntu host, dependency simulation required no additional libraries;
`ldd` resolved all libraries through Ubuntu, including PAM. The installed PAM
file is identical to Ubuntu's swaylock PAM file. Two headless Sway 1.9 outputs
rendered Verdant through Intel ARL OpenGL ES, with changing frames and a working
keyboard indicator. No real desktop was locked during these checks; password
authentication still needs the user's interactive verification.
