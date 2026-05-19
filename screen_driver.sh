xrandr --newmode "800x480_hdmi" 29.58 800 816 896 992 480 481 484 497 -HSync +Vsync || echo ""
xrandr --addmode HDMI-1 "800x480_hdmi" || echo ""
xrandr --output HDMI-1 --mode "800x480_hdmi" || echo ""

xrandr --output VGA-1 --mode "800x480_60.00" --output HDMI-1 --mode "800x480_hdmi" --same-as VGA-1 || echo ""
xrandr --output VGA-1 --mode "800x480_60.00" || echo ""

sudo iptables -t nat -A POSTROUTING -o wlp3s0 -j MASQUERADE; sudo iptables -A FORWARD -i enp2s0f0 -o wlp3s0 -j ACCEPT; sudo iptables -A FORWARD -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
