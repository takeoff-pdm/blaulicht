if [ "$USER" = "root" ]; then
    echo "Please dont run this installer as root."
    exit 1
fi

# #
# # BT-audio setup.
# #
#
# if [ -d bt-audio ]; then
#     cd bt-audio && git pull && cd ../
# else
#     git clone https://github.com/MikMuellerDev/bt-audio.git || exit 1
# fi
#
# cd bt-audio && sudo ./install.sh && cd ../ || exit 1

# #
# # Udev / ESP32 Setup.
# #
#
# sudo cp udev.rules /etc/udev/rules.d/99-ttyACM0.rules || exit 1
# sudo udevadm trigger || exit 1
# sudo udevadm control --reload  || exit 1

#
# Session login.
#

sudo apt install -y lightdm || exit 1
# sed "s/USER-PLACEHOLDER/${USER}/g" gdm3.conf | sudo tee /etc/gdm3/daemon.conf || exit 1
sudo cp ./lightdm.conf /etc/lightdm/lightdm.conf || exit 1

sudo systemctl enable lightdm || exit  1

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

sudo apt install -y wget jq libxkbcommon-x11-0 x11-xserver-utils psmisc xserver-xorg-input-all openbox obconf devilspie2 || exit 1
sudo killall blaulicht || echo "Blaulicht not running"

rm blaulicht-latest.tar.gz || echo "No junk yet"
rm blaulicht || echo "No junk yet..."
rm -r ./blaulicht-dist || echo "No junk yet..."

wget -qO- https://api.github.com/repos/takeoff-pdm/blaulicht/releases/latest \
  | jq -r '.assets[] | select(.name | endswith("-x86_64-unknown-linux-gnu.tar.gz")) | .browser_download_url' \
  | xargs wget -O blaulicht-latest.tar.gz
# wget 'http://.edu/mik/crav/releases/download/latest/crav' || exit 1
# sudo killall crav || echo "Crav is not running..."
tar xvf blaulicht-latest.tar.gz
mv ./blaulicht-dist/blaulicht ./blaulicht
sudo cp blaulicht /usr/bin/blaulicht || exit 1
sudo chmod +x /usr/bin/blaulicht || exit 1

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


sudo apt install -y pulseaudio pulseaudio-module-zeroconf pulseaudio-utils

SYSTEMD_BASE_PATH=~/.config/systemd/user
mkdir -p "${SYSTEMD_BASE_PATH}"
sudo cp ./pulse.sh "/usr/bin/pulse.sh" || exit 1
sudo chmod +x "/usr/bin/pulse.sh" || exit 1
cp ./pulse.service "${SYSTEMD_BASE_PATH}/pulse.service" || exit 1
systemctl --user enable pulse || exit 1

echo "blaulicht ALL=(ALL) NOPASSWD: /sbin/shutdown, /sbin/reboot" | sudo tee /etc/sudoers.d/blaulicht-shutdown
echo "blaulicht ALL=(ALL) NOPASSWD: /usr/bin/fans" | sudo tee /etc/sudoers.d/blaulicht-fans

sudo chmod 440 /etc/sudoers.d/blaulicht-shutdown
sudo chmod 440 /etc/sudoers.d/blaulicht-fans


#
#
# Remote desktop
#
#

sudo apt-get update
sudo apt-get install x2goserver x2goserver-xsession tmux -y

echo "Installation succeeded, please reboot."
