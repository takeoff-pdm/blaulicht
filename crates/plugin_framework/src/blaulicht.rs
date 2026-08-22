// Wasm imports
#[link(wasm_import_module = "blaulicht")]
extern "C" {
    fn bl_register_plugin_kind(
        plugin_id: i32,
        kind: i32,
        key_ptr: *const u8,
        key_len: i32,
        name_ptr: *const u8,
        name_len: i32,
    );

    fn log(plugin_id: u8, ptr: *const u8, len: usize, log_level: i32);
    fn sys(plugin_id: u8, ptr: *const u8, len: usize, output_ptr: *mut u8, output_len: usize);
    fn udp(
        target_addr_ptr: *const u8,
        target_addr_len: usize,
        body_ptr: *const u8,
        body_len: usize,
    );

    fn bl_open_midi_device(device_name_ptr: *const u8, device_name_len: usize) -> u8;
    fn bl_enumerate_midi_devices(buffer_ptr: *mut u8, buffer_len: usize) -> u32;
    fn bl_transmit_midi(device_id: u8, status: u8, data0: u8, data1: u8);
    fn bl_report_panic(plugin_id: u8, ptr: *const u8, len: usize);

    fn bl_open_serial_device(
        device_name_ptr: *const u8,
        device_name_len: usize,
        baud_rate: u32,
    ) -> u8;

    fn bl_open_udp_port(bind_port: u32) -> u8;

    fn bl_artnet_register_receiver(plugin_id: u8, addr_ptr: *const u8, addr_len: usize) -> u32;
    fn bl_artnet_unregister_receiver(plugin_id: u8, handle: u32) -> u32;
    fn bl_artnet_enumerate_receivers(buffer_ptr: *mut u8, buffer_len: usize) -> u32;
    fn bl_enumerate_serial_devices(buffer_ptr: *mut u8, buffer_len: usize) -> u32;

    fn bl_send_event(serialized_buf: *const u8, buf_len: usize);

    fn bl_save_plugin_state(plugin_id: u8, location: u8, data_ptr: *const u8, data_len: usize);
    fn bl_load_plugin_state(
        plugin_id: u8,
        location: u8,
        buffer_ptr: *mut u8,
        buffer_len: usize,
    ) -> u32;

    fn command_spawn(plugin_id: u8, cmd_ptr: *const u8, cmd_len: usize) -> u32;
    fn command_poll(handle: u32, output_ptr: *mut u8, output_capacity: usize) -> i32;

    fn controls_log(x: u8, y: u8, ptr: *const u8, len: usize);
    fn controls_set(x: u8, y: u8, value: bool);
    fn controls_config(x: u8, y: u8);

    // Egui UI host imports
    fn ui_begin(plugin_id: u8);
    fn ui_is_open(plugin_id: u8) -> i32;
    fn ui_maximize_screen(plugin_id: u8);
    fn ui_label(plugin_id: u8, ptr: *const u8, len: usize);
    fn ui_label_styled(plugin_id: u8, ptr: *const u8, len: usize, size: i32, monospace: i32);
    fn ui_set_max_width(plugin_id: u8, width: i32);
    fn ui_set_min_width(plugin_id: u8, width: i32);
    fn ui_separator(plugin_id: u8);
    fn ui_button(plugin_id: u8, ptr: *const u8, len: usize, id: u8);
    fn ui_button_styled(plugin_id: u8, ptr: *const u8, len: usize, id: u8, enabled: i32);
    fn ui_checkbox(plugin_id: u8, ptr: *const u8, len: usize, id: u8, checked: i32);
    fn ui_switch(plugin_id: u8, ptr: *const u8, len: usize, id: u8, value: i32);
    fn ui_slider(plugin_id: u8, ptr: *const u8, len: usize, id: u8, min: i32, max: i32, value: i32);
    fn ui_hfader(plugin_id: u8, ptr: *const u8, len: usize, id: u8, min: i32, max: i32, value: i32);
    fn ui_combo_box(
        plugin_id: u8,
        label_ptr: *const u8,
        label_len: usize,
        id: u8,
        options_ptr: *const u8,
        options_len: usize,
        selected: u8,
    );
    fn ui_text_edit(
        plugin_id: u8,
        label_ptr: *const u8,
        label_len: usize,
        id: u8,
        text_ptr: *const u8,
        text_len: usize,
    );
    fn ui_text_edit_multiline(
        plugin_id: u8,
        label_ptr: *const u8,
        label_len: usize,
        id: u8,
        text_ptr: *const u8,
        text_len: usize,
    );
    fn ui_begin_vertical(plugin_id: u8);
    fn ui_end_vertical(plugin_id: u8);
    fn ui_begin_horizontal(plugin_id: u8);
    fn ui_end_horizontal(plugin_id: u8);
    fn ui_painter_begin(plugin_id: u8, id: u8, width: i32, height: i32);
    fn ui_painter_rect(
        plugin_id: u8,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: i32,
        g: i32,
        b: i32,
        a: i32,
    );
    fn ui_painter_circle(
        plugin_id: u8,
        x: i32,
        y: i32,
        radius: i32,
        r: i32,
        g: i32,
        b: i32,
        a: i32,
    );
    fn ui_painter_rect_stroke(
        plugin_id: u8,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: i32,
        g: i32,
        b: i32,
        a: i32,
        thickness: i32,
    );
    fn ui_painter_circle_stroke(
        plugin_id: u8,
        x: i32,
        y: i32,
        radius: i32,
        r: i32,
        g: i32,
        b: i32,
        a: i32,
        thickness: i32,
    );
    fn ui_painter_line(
        plugin_id: u8,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        r: i32,
        g: i32,
        b: i32,
        a: i32,
        thickness: i32,
    );
    fn ui_painter_text(
        plugin_id: u8,
        x: i32,
        y: i32,
        size: i32,
        r: i32,
        g: i32,
        b: i32,
        a: i32,
        ptr: *const u8,
        len: usize,
    );
    fn ui_painter_cubic_bezier(
        plugin_id: u8,
        x1: i32,
        y1: i32,
        cx1: i32,
        cy1: i32,
        cx2: i32,
        cy2: i32,
        x2: i32,
        y2: i32,
        r: i32,
        g: i32,
        b: i32,
        a: i32,
        thickness: i32,
    );
    fn ui_painter_end(plugin_id: u8);
    fn ui_color_picker(plugin_id: u8, id: u8, r: i32, g: i32, b: i32, a: i32);
    fn ui_begin_frame(plugin_id: u8, id: u8);
    fn ui_begin_frame_styled(
        plugin_id: u8,
        id: u8,
        title_ptr: *const u8,
        title_len: usize,
        pad_x: i32,
        pad_y: i32,
        margin_x: i32,
        margin_y: i32,
    );
    fn ui_begin_frame_styled_border(
        plugin_id: u8,
        id: u8,
        title_ptr: *const u8,
        title_len: usize,
        pad_x: i32,
        pad_y: i32,
        margin_x: i32,
        margin_y: i32,
        border_r: i32,
        border_g: i32,
        border_b: i32,
        border_a: i32,
        border_thickness: i32,
    );
    fn ui_end_frame(plugin_id: u8);
    fn ui_begin_collapsing(plugin_id: u8, id: u8, ptr: *const u8, len: usize, default_open: i32);
    fn ui_end_collapsing(plugin_id: u8);
    fn ui_begin_tabs(plugin_id: u8, id: u8);
    fn ui_begin_tab(plugin_id: u8, tabs_id: u8, tab_id: u8, ptr: *const u8, len: usize);
    fn ui_end_tab(plugin_id: u8);
    fn ui_end_tabs(plugin_id: u8);
    fn ui_alert(plugin_id: u8, ptr: *const u8, len: usize, duration_ms: u32);
    fn ui_list_external_screens(plugin_id: u8, buffer_ptr: *mut u8, buffer_len: usize) -> u32;
    fn ui_create_external_screen(plugin_id: u8, width: i32, height: i32) -> i32;
    fn ui_remove_external_screen(plugin_id: u8, index: i32) -> i32;
}

pub(crate) fn register_plugin_kind(kind: i32, key: &str, name: &str) {
    unsafe {
        bl_register_plugin_kind(
            PLUGIN_ID.into(),
            kind,
            key.as_ptr(),
            key.len() as i32,
            name.as_ptr(),
            name.len() as i32,
        );
    }
}

pub fn bl_open_midi_device_safe(device_name: &str) -> u8 {
    unsafe { bl_open_midi_device(device_name.as_ptr(), device_name.len()) }
}

pub fn bl_enumerate_midi_devices_safe() -> Vec<String> {
    const MAX_BUFFER_SIZE: usize = 4096;
    let mut buffer = vec![0u8; MAX_BUFFER_SIZE];

    let written_len = unsafe { bl_enumerate_midi_devices(buffer.as_mut_ptr(), MAX_BUFFER_SIZE) };

    if written_len == 0 {
        return Vec::new();
    }

    let data = &buffer[..(written_len as usize).min(buffer.len())];
    let json_str = std::str::from_utf8(data).unwrap_or("[]");
    serde_json::from_str(json_str).unwrap_or_else(|_| Vec::new())
}

pub fn bl_open_serial_device_safe(device_name: &str, baud_rate: u32) -> u8 {
    unsafe { bl_open_serial_device(device_name.as_ptr(), device_name.len(), baud_rate) }
}

pub fn bl_open_udp_port_safe(bind_port: u16) -> u8 {
    unsafe { bl_open_udp_port(bind_port as u32) }
}

/// Register an Art-Net receiver owned by the calling plugin. The core will start
/// shipping rendered DMX universes to `addr` (format: `"ip:port"`, IPv4 only,
/// typically `"x.x.x.x:6454"`) on every DMX tick until unregistered.
///
/// Returns a handle (`> 0`) on success, or `0` on failure (bad address, address
/// already registered, etc.). The handle is opaque to the plugin and must be
/// passed to [`bl_artnet_unregister_receiver_safe`] to remove the receiver.
///
/// On plugin crash or reload, the host drops all receivers owned by the plugin
/// automatically — explicit cleanup is only required for graceful shutdowns.
pub fn bl_artnet_register_receiver_safe(addr: &str) -> u32 {
    unsafe { bl_artnet_register_receiver(PLUGIN_ID, addr.as_ptr(), addr.len()) }
}

/// Unregister a previously-registered Art-Net receiver by handle. Returns `true`
/// if a receiver was actually removed.
pub fn bl_artnet_unregister_receiver_safe(handle: u32) -> bool {
    if handle == 0 {
        return false;
    }
    unsafe { bl_artnet_unregister_receiver(PLUGIN_ID, handle) != 0 }
}

/// Enumerate all Art-Net receivers currently registered on the host (user-created
/// and plugin-owned alike). Useful for sanity-checking against duplicates or
/// stale self-owned handles when reconfiguring.
pub fn bl_artnet_enumerate_receivers_safe() -> Vec<ArtNetReceiverInfo> {
    const MAX_BUFFER_SIZE: usize = 4096;
    let mut buffer = vec![0u8; MAX_BUFFER_SIZE];

    let written_len =
        unsafe { bl_artnet_enumerate_receivers(buffer.as_mut_ptr(), MAX_BUFFER_SIZE) };

    if written_len == 0 {
        return Vec::new();
    }

    let data = &buffer[..(written_len as usize).min(buffer.len())];
    let json_str = std::str::from_utf8(data).unwrap_or("[]");
    serde_json::from_str(json_str).unwrap_or_else(|_| Vec::new())
}

pub fn bl_enumerate_serial_devices_safe() -> Vec<String> {
    const MAX_BUFFER_SIZE: usize = 4096;
    let mut buffer = vec![0u8; MAX_BUFFER_SIZE];

    let written_len = unsafe { bl_enumerate_serial_devices(buffer.as_mut_ptr(), MAX_BUFFER_SIZE) };

    if written_len == 0 {
        return Vec::new();
    }

    let data = &buffer[..(written_len as usize).min(buffer.len())];
    let json_str = std::str::from_utf8(data).unwrap_or("[]");
    serde_json::from_str(json_str).unwrap_or_else(|_| Vec::new())
}

pub fn bl_list_external_screens_safe() -> Vec<ExternalScreenInfo> {
    const MAX_BUFFER_SIZE: usize = 4096;
    let mut buffer = vec![0u8; MAX_BUFFER_SIZE];

    let written_len =
        unsafe { ui_list_external_screens(PLUGIN_ID, buffer.as_mut_ptr(), MAX_BUFFER_SIZE) };

    if written_len == 0 {
        return Vec::new();
    }

    let data = &buffer[..(written_len as usize).min(buffer.len())];
    let json_str = std::str::from_utf8(data).unwrap_or("[]");
    serde_json::from_str(json_str).unwrap_or_else(|_| Vec::new())
}

pub fn bl_create_external_screen_safe(width: u32, height: u32) -> bool {
    let width = width.min(i32::MAX as u32) as i32;
    let height = height.min(i32::MAX as u32) as i32;
    unsafe { ui_create_external_screen(PLUGIN_ID, width, height) != 0 }
}

pub fn bl_remove_external_screen_safe(index: u32) -> bool {
    let index = index.min(i32::MAX as u32) as i32;
    unsafe { ui_remove_external_screen(PLUGIN_ID, index) != 0 }
}

pub fn bl_alert_safe(text: &str, duration_ms: u32) {
    unsafe { ui_alert(PLUGIN_ID, text.as_ptr(), text.len(), duration_ms) }
}

pub fn report_panic(msg: &str) {
    unsafe { bl_report_panic(PLUGIN_ID, msg.as_ptr(), msg.len()) }
}

pub fn bl_transmit_midi_safe(device: u8, status: u8, data0: u8, data1: u8) {
    unsafe { bl_transmit_midi(device, status, data0, data1) }
}

/// Log a string to the BL output

pub static mut PLUGIN_ID: u8 = 0;

pub fn bl_log(msg: &str, level: LogLevel) {
    unsafe { log(PLUGIN_ID, msg.as_ptr(), msg.len(), level.into()) }
}

pub fn system(cmd: &str) -> String {
    const OUTPUT_BUFFER_SIZE: usize = 64 * 1024;
    let mut buffer = vec![0u8; OUTPUT_BUFFER_SIZE];

    unsafe {
        sys(
            PLUGIN_ID,
            cmd.as_ptr(),
            cmd.len(),
            buffer.as_mut_ptr(),
            buffer.len(),
        );
    }

    let len = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..len]).into_owned()
}

pub enum CommandPollResult {
    Running,
    Finished(String),
    Failed(String),
}

pub fn command_spawn_bg(cmd: &str) -> u32 {
    unsafe { command_spawn(PLUGIN_ID, cmd.as_ptr(), cmd.len()) }
}

pub fn command_poll_result(handle: u32) -> CommandPollResult {
    const OUTPUT_BUFFER_SIZE: usize = 64 * 1024;
    let mut buffer = vec![0u8; OUTPUT_BUFFER_SIZE];

    let status = unsafe { command_poll(handle, buffer.as_mut_ptr(), buffer.len()) };

    match status {
        0 => CommandPollResult::Running,
        1 => {
            let len = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
            CommandPollResult::Finished(String::from_utf8_lossy(&buffer[..len]).into_owned())
        }
        _ => {
            let len = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
            CommandPollResult::Failed(String::from_utf8_lossy(&buffer[..len]).into_owned())
        }
    }
}

pub fn send_event(event: ControlEvent) {
    let serialized = event.serialize();
    unsafe { bl_send_event(serialized.as_ptr(), serialized.len()) };
}

pub fn save_plugin_state(location: PluginStateLocation, data: &str) {
    unsafe { bl_save_plugin_state(PLUGIN_ID, location.into(), data.as_ptr(), data.len()) }
}

pub fn load_plugin_state(location: PluginStateLocation) -> Option<String> {
    const MAX_BUFFER_SIZE: usize = 1024 * 100;
    let mut buffer = vec![0u8; MAX_BUFFER_SIZE];

    let written_len = unsafe {
        bl_load_plugin_state(
            PLUGIN_ID,
            location.into(),
            buffer.as_mut_ptr(),
            MAX_BUFFER_SIZE,
        )
    };

    if written_len == 0 {
        return None;
    }

    let data = &buffer[..(written_len as usize).min(buffer.len())];
    Some(String::from_utf8_lossy(data).into_owned())
}

pub fn send_udp(addr: &str, body: &[u8]) {
    unsafe { udp(addr.as_ptr(), addr.len(), body.as_ptr(), body.len()) }
}

pub fn bl_controls_log(x: u8, y: u8, msg: &str) {
    unsafe { controls_log(x, y, msg.as_ptr(), msg.len()) }
}

pub fn bl_controls_set(x: u8, y: u8, value: bool) {
    unsafe { controls_set(x, y, value) }
}

pub fn bl_controls_config(x: u8, y: u8) {
    println!("[MATRIX] Configuring controls to ({}, {})...", x, y);
    unsafe {
        controls_config(x, y);
    }
}

// ---- UI (egui) helpers ----

pub mod ui {
    use super::{
        ui_begin as host_ui_begin, ui_begin_collapsing as host_ui_begin_collapsing,
        ui_begin_frame as host_ui_begin_frame, ui_begin_frame_styled as host_ui_begin_frame_styled,
        ui_begin_frame_styled_border as host_ui_begin_frame_styled_border,
        ui_begin_horizontal as host_ui_begin_horizontal, ui_begin_tab as host_ui_begin_tab,
        ui_begin_tabs as host_ui_begin_tabs, ui_begin_vertical as host_ui_begin_vertical,
        ui_button as host_ui_button, ui_button_styled as host_ui_button_styled,
        ui_checkbox as host_ui_checkbox, ui_color_picker as host_ui_color_picker,
        ui_combo_box as host_ui_combo_box, ui_end_collapsing as host_ui_end_collapsing,
        ui_end_frame as host_ui_end_frame, ui_end_horizontal as host_ui_end_horizontal,
        ui_end_tab as host_ui_end_tab, ui_end_tabs as host_ui_end_tabs,
        ui_end_vertical as host_ui_end_vertical, ui_hfader as host_ui_hfader,
        ui_is_open as host_ui_is_open, ui_label as host_ui_label,
        ui_label_styled as host_ui_label_styled, ui_maximize_screen as host_ui_maximize_screen,
        ui_painter_begin as host_ui_painter_begin, ui_painter_circle as host_ui_painter_circle,
        ui_painter_circle_stroke as host_ui_painter_circle_stroke,
        ui_painter_cubic_bezier as host_ui_painter_cubic_bezier,
        ui_painter_end as host_ui_painter_end, ui_painter_line as host_ui_painter_line,
        ui_painter_rect as host_ui_painter_rect,
        ui_painter_rect_stroke as host_ui_painter_rect_stroke,
        ui_painter_text as host_ui_painter_text, ui_separator as host_ui_separator,
        ui_set_max_width as host_ui_set_max_width, ui_set_min_width as host_ui_set_min_width,
        ui_slider as host_ui_slider, ui_switch as host_ui_switch,
        ui_text_edit as host_ui_text_edit, ui_text_edit_multiline as host_ui_text_edit_multiline,
        PLUGIN_ID,
    };
    use std::time::Duration;

    pub fn begin() {
        unsafe { host_ui_begin(unsafe { PLUGIN_ID }) };
    }

    pub fn is_open() -> bool {
        unsafe { host_ui_is_open(unsafe { PLUGIN_ID }) != 0 }
    }

    pub fn maximize_screen() {
        unsafe { host_ui_maximize_screen(unsafe { PLUGIN_ID }) };
    }

    pub fn alert(text: &str) {
        super::bl_alert_safe(text, 2_000);
    }

    pub fn alert_for(text: &str, duration: Duration) {
        let duration_ms = duration.as_millis().min(u32::MAX as u128) as u32;
        super::bl_alert_safe(text, duration_ms);
    }

    pub fn external_screens() -> Vec<super::ExternalScreenInfo> {
        super::bl_list_external_screens_safe()
    }

    pub fn create_external_screen(width: u32, height: u32) -> bool {
        super::bl_create_external_screen_safe(width, height)
    }

    pub fn remove_external_screen(index: u32) -> bool {
        super::bl_remove_external_screen_safe(index)
    }

    pub fn label(text: &str) {
        unsafe { host_ui_label(unsafe { PLUGIN_ID }, text.as_ptr(), text.len()) };
    }

    pub fn label_styled(text: &str, size: i32, monospace: bool) {
        unsafe {
            host_ui_label_styled(
                unsafe { PLUGIN_ID },
                text.as_ptr(),
                text.len(),
                size,
                if monospace { 1 } else { 0 },
            )
        };
    }

    pub fn set_max_width(width: i32) {
        unsafe { host_ui_set_max_width(unsafe { PLUGIN_ID }, width) };
    }

    pub fn set_min_width(width: i32) {
        unsafe { host_ui_set_min_width(unsafe { PLUGIN_ID }, width) };
    }

    pub fn separator() {
        unsafe { host_ui_separator(unsafe { PLUGIN_ID }) };
    }

    /// Enqueue a button with a stable `id` byte. When clicked on host, a ControlEvent::MiscEvent will be sent back with descriptor=id, value=1.
    pub fn button(label: &str, id: u8) {
        unsafe { host_ui_button(unsafe { PLUGIN_ID }, label.as_ptr(), label.len(), id) };
    }

    /// Enqueue a styled button using core ButtonSize::Medium.
    pub fn button_styled(label: &str, id: u8, enabled: bool) {
        unsafe {
            host_ui_button_styled(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                if enabled { 1 } else { 0 },
            )
        };
    }

    pub fn combo_box(label: &str, id: u8, options: &[String], selected: u8) {
        let joined = options.join("\n");
        unsafe {
            host_ui_combo_box(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                joined.as_ptr(),
                joined.len(),
                selected,
            )
        };
    }

    pub fn checkbox(label: &str, id: u8, checked: bool) {
        unsafe {
            host_ui_checkbox(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                if checked { 1 } else { 0 },
            )
        };
    }

    pub fn switch(label: &str, id: u8, value: bool) {
        unsafe {
            host_ui_switch(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                if value { 1 } else { 0 },
            )
        };
    }

    pub fn slider(label: &str, id: u8, min: u8, max: u8, value: u8) {
        unsafe {
            host_ui_slider(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                min as i32,
                max as i32,
                value as i32,
            )
        };
    }

    pub fn hfader(label: &str, id: u8, min: u8, max: u8, value: u8) {
        unsafe {
            host_ui_hfader(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                min as i32,
                max as i32,
                value as i32,
            )
        };
    }

    pub fn text_edit(label: &str, id: u8, text: &str) {
        unsafe {
            host_ui_text_edit(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                text.as_ptr(),
                text.len(),
            )
        };
    }

    pub fn text_edit_multiline(label: &str, id: u8, text: &str) {
        unsafe {
            host_ui_text_edit_multiline(
                unsafe { PLUGIN_ID },
                label.as_ptr(),
                label.len(),
                id,
                text.as_ptr(),
                text.len(),
            )
        };
    }

    pub fn begin_vertical() {
        unsafe { host_ui_begin_vertical(unsafe { PLUGIN_ID }) };
    }
    pub fn end_vertical() {
        unsafe { host_ui_end_vertical(unsafe { PLUGIN_ID }) };
    }
    pub fn begin_horizontal() {
        unsafe { host_ui_begin_horizontal(unsafe { PLUGIN_ID }) };
    }
    pub fn end_horizontal() {
        unsafe { host_ui_end_horizontal(unsafe { PLUGIN_ID }) };
    }
    pub fn painter_begin(id: u8, width: i32, height: i32) {
        unsafe { host_ui_painter_begin(unsafe { PLUGIN_ID }, id, width, height) };
    }
    pub fn painter_rect(x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8, a: u8) {
        unsafe {
            host_ui_painter_rect(
                unsafe { PLUGIN_ID },
                x,
                y,
                w,
                h,
                r as i32,
                g as i32,
                b as i32,
                a as i32,
            )
        };
    }
    pub fn painter_circle(x: i32, y: i32, radius: i32, r: u8, g: u8, b: u8, a: u8) {
        unsafe {
            host_ui_painter_circle(
                unsafe { PLUGIN_ID },
                x,
                y,
                radius,
                r as i32,
                g as i32,
                b as i32,
                a as i32,
            )
        };
    }
    pub fn painter_end() {
        unsafe { host_ui_painter_end(unsafe { PLUGIN_ID }) };
    }

    pub fn painter_line(
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        thickness: i32,
    ) {
        unsafe {
            host_ui_painter_line(
                unsafe { PLUGIN_ID },
                x1,
                y1,
                x2,
                y2,
                r as i32,
                g as i32,
                b as i32,
                a as i32,
                thickness,
            )
        };
    }

    pub fn painter_text(x: i32, y: i32, size: i32, r: u8, g: u8, b: u8, a: u8, text: &str) {
        unsafe {
            host_ui_painter_text(
                unsafe { PLUGIN_ID },
                x,
                y,
                size,
                r as i32,
                g as i32,
                b as i32,
                a as i32,
                text.as_ptr(),
                text.len(),
            )
        };
    }

    pub fn painter_rect_stroke(
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        thickness: i32,
    ) {
        unsafe {
            host_ui_painter_rect_stroke(
                unsafe { PLUGIN_ID },
                x,
                y,
                w,
                h,
                r as i32,
                g as i32,
                b as i32,
                a as i32,
                thickness,
            )
        };
    }
    pub fn painter_circle_stroke(
        x: i32,
        y: i32,
        radius: i32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        thickness: i32,
    ) {
        unsafe {
            host_ui_painter_circle_stroke(
                unsafe { PLUGIN_ID },
                x,
                y,
                radius,
                r as i32,
                g as i32,
                b as i32,
                a as i32,
                thickness,
            )
        };
    }
    pub fn painter_cubic_bezier(
        x1: i32,
        y1: i32,
        cx1: i32,
        cy1: i32,
        cx2: i32,
        cy2: i32,
        x2: i32,
        y2: i32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
        thickness: i32,
    ) {
        unsafe {
            host_ui_painter_cubic_bezier(
                unsafe { PLUGIN_ID },
                x1,
                y1,
                cx1,
                cy1,
                cx2,
                cy2,
                x2,
                y2,
                r as i32,
                g as i32,
                b as i32,
                a as i32,
                thickness,
            )
        };
    }

    pub fn color_picker(id: u8, r: u8, g: u8, b: u8, a: u8) {
        unsafe {
            host_ui_color_picker(
                unsafe { PLUGIN_ID },
                id,
                r as i32,
                g as i32,
                b as i32,
                a as i32,
            )
        };
    }

    pub fn begin_frame(id: u8) {
        unsafe { host_ui_begin_frame(unsafe { PLUGIN_ID }, id) };
    }
    pub fn begin_frame_styled(
        id: u8,
        title: &str,
        pad_x: i32,
        pad_y: i32,
        margin_x: i32,
        margin_y: i32,
    ) {
        unsafe {
            host_ui_begin_frame_styled(
                unsafe { PLUGIN_ID },
                id,
                title.as_ptr(),
                title.len(),
                pad_x,
                pad_y,
                margin_x,
                margin_y,
            )
        };
    }

    pub fn begin_frame_styled_border(
        id: u8,
        title: &str,
        pad_x: i32,
        pad_y: i32,
        margin_x: i32,
        margin_y: i32,
        border_r: u8,
        border_g: u8,
        border_b: u8,
        border_a: u8,
        border_thickness: i32,
    ) {
        unsafe {
            host_ui_begin_frame_styled_border(
                unsafe { PLUGIN_ID },
                id,
                title.as_ptr(),
                title.len(),
                pad_x,
                pad_y,
                margin_x,
                margin_y,
                border_r as i32,
                border_g as i32,
                border_b as i32,
                border_a as i32,
                border_thickness,
            )
        };
    }
    pub fn end_frame() {
        unsafe { host_ui_end_frame(unsafe { PLUGIN_ID }) };
    }
    pub fn begin_collapsing(id: u8, title: &str, default_open: bool) {
        unsafe {
            host_ui_begin_collapsing(
                unsafe { PLUGIN_ID },
                id,
                title.as_ptr(),
                title.len(),
                if default_open { 1 } else { 0 },
            )
        };
    }
    pub fn end_collapsing() {
        unsafe { host_ui_end_collapsing(unsafe { PLUGIN_ID }) };
    }
    pub fn begin_tabs(id: u8) {
        unsafe { host_ui_begin_tabs(unsafe { PLUGIN_ID }, id) };
    }
    pub fn begin_tab(tabs_id: u8, tab_id: u8, title: &str) {
        unsafe {
            host_ui_begin_tab(
                unsafe { PLUGIN_ID },
                tabs_id,
                tab_id,
                title.as_ptr(),
                title.len(),
            )
        };
    }
    pub fn end_tab() {
        unsafe { host_ui_end_tab(unsafe { PLUGIN_ID }) };
    }
    pub fn end_tabs() {
        unsafe { host_ui_end_tabs(unsafe { PLUGIN_ID }) };
    }
}

// const PAGE_SIZE: usize = 65536;

#[doc(hidden)]
pub unsafe fn _get_array(array_pointer: *mut u8, array_length: usize) -> &'static mut [u8] {
    // Safety: This is unsafe because we're dealing with raw pointers.
    if array_pointer.is_null() {
        return &mut [];
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(array_pointer, array_length) };

    slice
}

#[doc(hidden)]
pub unsafe fn _get_array_u32(array_pointer: *const u32, array_length: usize) -> &'static [u32] {
    // Safety: This is unsafe because we're dealing with raw pointers.
    if array_pointer.is_null() {
        return &[];
    }
    let slice = unsafe { std::slice::from_raw_parts(array_pointer, array_length) };

    slice
}

pub mod prelude {

    #[macro_export]
    macro_rules! println {
            () => {
                use blaulicht_shared::LogLevel;
                $crate::blaulicht::bl_log!("", LogLevel::Info);
            };
            ($($arg:tt)*) => {{
                use blaulicht_shared::LogLevel;
                $crate::blaulicht::bl_log(&format!($($arg)*), LogLevel::Info);
            }};
        }
    pub use println;

    #[macro_export]
    macro_rules! printc {
            () => {
                $crate::blaulicht::bl_controls_log!("");
            };
            ($x: expr, $y:expr, $($arg:tt)*) => {{
                $crate::blaulicht::bl_controls_log($x, $y, &format!($($arg)*));
            }};
        }
    pub use printc;

    #[macro_export]
    macro_rules! print {
        ($($arg:tt)*) => {
            panic!("'print!' not supported in Blaulicht!");
        };
    }

    #[macro_export]
    macro_rules! dbg {
        ($($arg:tt)*) => {
            panic!("'dbg!' not supported in Blaulicht!");
        };
    }

    #[macro_export]
    macro_rules! smidi {
        ($tuple: expr, $value: expr) => {
            blaulicht::bl_midi_safe(0, $tuple.0, $tuple.1, $value);
            blaulicht::bl_midi_safe(1, $tuple.0, $tuple.1, $value);
        };
    }
}

// pub fn fmod(a: f64, b: f64) -> f64 {
//     a - b * (a / b).trunc()
// }

#[macro_export]
macro_rules! elapsed {
    ($input: expr, $time: expr) => {
        $input.clock.wrapping_sub($time)
    };
}

use std::fmt::Display;

use blaulicht_shared::{
    ArtNetReceiverInfo, ControlEvent, ExternalScreenInfo, LogLevel, PluginStateLocation,
};
pub use elapsed;

#[macro_export]
macro_rules! nelapsed {
    ($input: expr, $time: expr) => {
        ($input.clock as i32).wrapping_sub($time as i32)
    };
}

pub use nelapsed;
