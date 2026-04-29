  local name = get_window_name() or ""

if name == "bl_ext_0" then
    debug_print("[DS2] Blaulicht external screen")
    -- Replace 1920 with the X offset of monitor 2 from `xrandr --query`
    set_window_position(800, 0)
    maximize()
    undecorate_window()
elseif name == "blaulicht" then
    debug_print("[DS2] Blaulicht main screen")
    maximize()
    undecorate_window()
else
    debug_print("[DS2] unknown window: " .. tostring(name))
end
