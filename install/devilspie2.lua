local name = get_window_name() or ""
local ext_prefix = "bl_ext_"

if name:sub(1, #ext_prefix) == ext_prefix then
    debug_print("[DS2] Blaulicht external screen")
    -- Default fallback: place the external window on the HDMI screen above VGA.
    set_window_geometry(0, 0, 1920, 1080)
    undecorate_window()
elseif name == "blaulicht" then
    debug_print("[DS2] Blaulicht main screen")
    maximize()
    undecorate_window()
else
    debug_print("[DS2] unknown window: " .. tostring(name))
end
