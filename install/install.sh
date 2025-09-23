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
# Install Blaulicht.
#
#

sudo killall blaulicht || echo "Blaulicht not running"
sudo apt install -y wget jq || exit 1

rm blaulicht || echo "No junk yet..."
wget -qO- https://api.github.com/repos/takeoff-pdm/blaulicht/releases/latest \
  | jq -r '.assets[] | select(.name | endswith("-x86_64-unknown-linux-gnu.tar.gz")) | .browser_download_url' \
  | xargs wget -O blaulicht-latest.tar.gz
# wget 'http://.edu/mik/crav/releases/download/latest/crav' || exit 1
# sudo killall crav || echo "Crav is not running..."
tar xvf blaulicht-latest.tar.gz
mv ./blaulicht-dist/blaulicht ./blaulicht
sudo cp blaulicht /usr/bin/blaulicht || exit 1
sudo chmod +x /usr/bin/blaulicht || exit 1

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
sudo chmod 440 /etc/sudoers.d/blaulicht-shutdown

echo "Installation succeeded, please reboot."
