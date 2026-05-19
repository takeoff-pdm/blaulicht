#
# TODO: do CPU-specific optimizations for power states / never turobo
#

# Source - https://stackoverflow.com/q
# Posted by Aleksandr Murashov
# Retrieved 2026-01-03, License - CC BY-SA 3.0
set -o errexit
set -o nounset
set -o pipefail

# NOTE: should be changed
# TODO: add flags to setup the parameters of the installer
VNC_PASSWORD="blaulicht"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ "$USER" = "root" ]; then
    echo "Please dont run this installer as root. You will be prompted for your sudo password."
    exit 1
fi

cd "${SCRIPT_DIR}"

blaulicht_binary() {
    sudo killall blaulicht || echo "[kill old instance] Blaulicht not running"

    rm blaulicht-latest.tar.gz || echo "[remove old files] No junk."
    rm blaulicht || echo "[remove old files] No junk."
    rm -r ./blaulicht-dist || echo "[remove old files] No junk."

    wget --no-check-certificate -qO- https://api.github.com/repos/takeoff-pdm/blaulicht/releases/latest |
        jq -r '.assets[] | select(.name | endswith("-x86_64-all-linux-gnu.tar.gz")) | .browser_download_url' |
        xargs wget -O blaulicht-latest.tar.gz || echo "[download] ERROR: Download failed, trying to setup everything else"

    tar xvf blaulicht-latest.tar.gz || exit 2
    mv ./blaulicht-dist/blaulicht-x64-haswell ./blaulicht || exit 2
    sudo cp blaulicht /usr/bin/blaulicht || exit 2
    sudo chmod +x /usr/bin/blaulicht || exit 2
}

install_runtime_files() {
    mkdir -p ~/.config/openbox
    cp ./openbox-autostart ~/.config/openbox/autostart || exit 2
    chmod +x ~/.config/openbox/autostart || exit 2

    mkdir -p ~/.config/devilspie2
    cp ./devilspie2.lua ~/.config/devilspie2/blaulicht.lua || exit 2

    sudo cp ./blaulicht.desktop /usr/share/xsessions/ || exit 2
    sudo chmod +x /usr/share/xsessions/blaulicht.desktop || exit 2

    sudo cp ./blaulicht.sh /usr/bin/blaulicht.sh || exit 2
    sudo chmod +x /usr/bin/blaulicht.sh || exit 2

    sudo cp ./generate-devilspie2.sh /usr/bin/generate-devilspie2.sh || exit 2
    sudo chmod +x /usr/bin/generate-devilspie2.sh || exit 2

    sudo cp ./rescue.sh /usr/bin/rescue.sh || exit 2
    sudo chmod +x /usr/bin/rescue.sh || exit 2

    sudo cp ./shutdown.sh /usr/bin/shutdown.sh || exit 2
    sudo chmod +x /usr/bin/shutdown.sh || exit 2

    sudo cp ./fans.sh /usr/bin/fans || exit 2
    sudo cp ./limits.conf /etc/security/limits.d/99-blaulicht-thread-priority.conf || exit 2
}

#
# Check if we need to install or update
#

subcommand="${1:-}"

case "$subcommand" in
install)
    echo "Running install..."
    ;;
upgrade)
    echo "Running upgrade..."
    blaulicht_binary
    install_runtime_files
    exit 0
    ;;
*)
    echo "Usage: $0 {install|upgrade}" >&2
    exit 1
    ;;
esac

#
# Session login.
#

# Delete old session files.
sudo mkdir -p /usr/share/xsessions/ || echo "[create xsessions dir] WARNING: Command failed."
sudo find /usr/share/xsessions/ ! -name openbox.desktop ! -name blaulicht.desktop ! -name lightdm-xsession.desktop -maxdepth 1 -type f -delete

# Delete gnome if installed.
sudo apt purge gnome-session gnome-shell -y || echo "[remove gnome] WARNING: Command failed."

# TODO: detect if installed before installing
# Delete appamor and other bluat
sudo systemctl stop apparmor || echo "[remove appamor] No appamor"
sudo systemctl disable apparmor || echo "[remove appamor] No appamor"
sudo apt purge -y apparmor || echo "[remove appamor] No appamor"


sudo systemctl stop cups cups-browsed || echo "[remove cups] No cups"
sudo systemctl disable cups cups-browsed || echo "[remove cups] No cups"
sudo apt purge -y cups* || echo "[remove cups] No cups"

sudo systemctl stop wpa_supplicant || echo "[remove WPA supplicant] Not installed"
sudo systemctl disable wpa_supplicant  || echo "[remove WPA supplicant] Not installed"
sudo apt purge -y wpasupplicant || echo "[remove WPA supplicant] Not installed"


sudo apt install -y lightdm || exit 2
sudo cp ./lightdm.conf /etc/lightdm/lightdm.conf || exit 2

sudo dpkg-reconfigure -fnoninteractive lightdm || exit 2
sudo systemctl enable lightdm || exit 2

#
# Install terminal
#

sudo apt install -y xterm || exit 2

#
# Install fan driver
#

#
# Install Blaulicht.
#
#

sudo apt install -y rsync curl btop wget jq libxkbcommon-x11-0 x11-xserver-utils psmisc xserver-xorg-input-all openbox obconf devilspie2 || exit 2

blaulicht_binary

install_runtime_files

#
# Rescue
#

sudo apt install lm-sensors -y || exit 2
sudo sensors-detect --auto || echo "[detect sensors] WARNING: Command failed"

#
# Pulseaudio.
# NOTE: this is to be installed after the gnome software so that gnome does not remove pulseaudio in favour of pipewire.
#

sudo apt install -y pipewire pipewire-pulse wireplumber || exit 2

echo "blaulicht ALL=(ALL) NOPASSWD: /sbin/shutdown, /sbin/reboot" | sudo tee /etc/sudoers.d/blaulicht-shutdown
echo "blaulicht ALL=(ALL) NOPASSWD: /usr/bin/fans" | sudo tee /etc/sudoers.d/blaulicht-fans

sudo chmod 440 /etc/sudoers.d/blaulicht-shutdown || exit 2
sudo chmod 440 /etc/sudoers.d/blaulicht-fans || exit 2

#
# Thread priority adjustments
#

#
#
# Remote desktop
#
#

sudo apt-get update || exit 2

sudo apt-get install tmux x11vnc -y || exit 2

mkdir -p ~/.vnc || exit 2
echo "$VNC_PASSWORD" >~/.vnc/passwd.txt
chmod 600 ~/.vnc/passwd.txt || exit 2

# Cleanup.
sudo apt autoremove -y || echo "[cleanup] Failed"

echo "######################################"
echo "Installation succeeded, please reboot."
echo "######################################"
