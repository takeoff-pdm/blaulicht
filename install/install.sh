# TODO: lightdm do not use gnome!

# Source - https://stackoverflow.com/q
# Posted by Aleksandr Murashov
# Retrieved 2026-01-03, License - CC BY-SA 3.0
set -o errexit
set -o nounset
set -o pipefail

# NOTE: should be changed
VNC_PASSWORD="blaulicht"

if [ "$USER" = "root" ]; then
    echo "Please dont run this installer as root."
    exit 1
fi

blaulicht_binary() {
    sudo killall blaulicht || echo "Blaulicht not running"

    rm blaulicht-latest.tar.gz || echo "No junk yet"
    rm blaulicht || echo "No junk yet..."
    rm -r ./blaulicht-dist || echo "No junk yet..."

    wget --no-check-certificate -qO- https://api.github.com/repos/takeoff-pdm/blaulicht/releases/latest |
        jq -r '.assets[] | select(.name | endswith("-x86_64-unknown-linux-gnu.tar.gz")) | .browser_download_url' |
        xargs wget -O blaulicht-latest.tar.gz || echo "WARNING: Download"

    tar xvf blaulicht-latest.tar.gz
    mv ./blaulicht-dist/blaulicht ./blaulicht
    sudo cp blaulicht /usr/bin/blaulicht || exit 1
    sudo chmod +x /usr/bin/blaulicht || exit 1
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
sudo mkdir -p /usr/share/xsessions/ || echo ""
sudo find /usr/share/xsessions/ ! -name openbox.desktop ! -name blaulicht.desktop ! -name lightdm-xsession.desktop -maxdepth 1 -type f -delete

# Delete gnome if installed.
sudo apt purge gnome-session gnome-shell -y || echo "Gnome removal completed with error"

# TODO: detect if installed before installing
# Delete appamor and other bluat
sudo systemctl stop apparmor || echo "No appamor"
sudo systemctl disable apparmor || echo "No appamor"
sudo apt purge -y apparmor || echo "Remove appamor"


sudo systemctl stop cups cups-browsed || echo "CUPS"
sudo systemctl disable cups cups-browsed || echo "CUPS"
sudo apt purge -y cups* || echo "CUPS"

sudo systemctl stop wpa_supplicant || echo "WPA"
sudo systemctl disable wpa_supplicant  || echo "WPA"
sudo apt purge -y wpasupplicant || echo "WPA"


sudo apt install -y lightdm || exit 1
# sed "s/USER-PLACEHOLDER/${USER}/g" gdm3.conf | sudo tee /etc/gdm3/daemon.conf || exit 1
sudo cp ./lightdm.conf /etc/lightdm/lightdm.conf || exit 1

sudo dpkg-reconfigure -fnoninteractive lightdm
sudo systemctl enable lightdm || exit 1

#
# Install terminal
#

sudo apt install -y xterm || exit 1

#
# Install fan driver
#

sudo cp ./fans.sh /usr/bin/fans

#
# Install Blaulicht.
#
#

sudo apt install -y rsync wget jq libxkbcommon-x11-0 x11-xserver-utils psmisc xserver-xorg-input-all openbox obconf devilspie2 || exit 1

blaulicht_binary

mkdir -p ~/.config/openbox
cp ./openbox-autostart ~/.config/openbox/autostart || exit 1
chmod +x ~/.config/openbox/autostart || exit 1

mkdir -p ~/.config/devilspie2
cp ./blaulicht.lua ~/.config/devilspie2/

# TOOD: autostart?
# AUTOSTART_BASE_DIR=~/.config/autostart/
# mkdir -p "${AUTOSTART_BASE_DIR}"
# cp ./crav.desktop "${AUTOSTART_BASE_DIR}" || exit 1
# sudo cp ./crav.desktop "/usr/share/applications/" || exit 1

sudo cp ./blaulicht.desktop /usr/share/xsessions/
sudo chmod +x /usr/share/xsessions/blaulicht.desktop

sudo cp blaulicht.sh /usr/bin/blaulicht.sh || exit 1
sudo chmod +x /usr/bin/blaulicht.sh || exit 1

#
# Rescue
#

sudo apt install lm-sensors -y
sudo sensors-detect --auto

sudo cp rescue.sh /usr/bin/rescue.sh || exit 1
sudo chmod +x /usr/bin/rescue.sh || exit 1

sudo cp shutdown.sh /usr/bin/shutdown.sh || exit 1
sudo chmod +x /usr/bin/shutdown.sh || exit 1

#
# Pulseaudio.
# NOTE: this is to be installed after the gnome software so that gnome does not remove pulseaudio in favour of pipewire.
#

# sudo apt install -y pulseaudio pulseaudio-module-zeroconf pulseaudio-utils
sudo apt install -y pipewire pipewire-pulse wireplumber

# SYSTEMD_BASE_PATH=~/.config/systemd/user
# mkdir -p "${SYSTEMD_BASE_PATH}"
# sudo cp ./pulse.sh "/usr/bin/pulse.sh" || exit 1
# sudo chmod +x "/usr/bin/pulse.sh" || exit 1
# cp ./pulse.service "${SYSTEMD_BASE_PATH}/pulse.service" || exit 1
# systemctl --user enable pulse || exit 1

echo "blaulicht ALL=(ALL) NOPASSWD: /sbin/shutdown, /sbin/reboot" | sudo tee /etc/sudoers.d/blaulicht-shutdown
echo "blaulicht ALL=(ALL) NOPASSWD: /usr/bin/fans" | sudo tee /etc/sudoers.d/blaulicht-fans

sudo chmod 440 /etc/sudoers.d/blaulicht-shutdown
sudo chmod 440 /etc/sudoers.d/blaulicht-fans

#
# Thread priority adjustments
#

sudo cp ./limits.conf /etc/security/limits.d/99-blaulicht-thread-priority.conf || exit 1

#
#
# Remote desktop
#
#

sudo apt-get update

sudo apt-get install tmux x11vnc -y

mkdir -p ~/.vnc
echo "$VNC_PASSWORD" >~/.vnc/passwd.txt
chmod 600 ~/.vnc/passwd.txt
# Cleanup.
sudo apt autoremove -y

echo "######################################"
echo "Installation succeeded, please reboot."
echo "######################################"
