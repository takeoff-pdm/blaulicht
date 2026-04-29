if get_window_name() == "blaulicht_ext: 0" then
    -- Move to external monitor (positioned to the right at x=1920)
    set_window_position(800.0, 0)

    -- Optionally set size too
    -- set_window_size(1280, 720)

    maximize();
    undecorate_window();
else
    maximize();
    undecorate_window();
end

