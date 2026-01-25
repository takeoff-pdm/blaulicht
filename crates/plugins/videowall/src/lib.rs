use blaulicht_plugin_framework as bpf;
use blaulicht_plugin_framework::Plugin;
use blaulicht_shared::{
    misc_event::videowall::{
        REQUEST_STATUS_REFRESH, SET_BRIGHTNESS, SET_FRY, SET_ROTATION, SET_SPEED, SET_VIDEO_INDEX,
    },
    ControlEvent, ControlEventMessage, PluginUiEvent, TickInput,
};
use map_range::MapRange;
use serde::Deserialize;

const UDP_ENDPOINT: &str = "10.10.25.98:1234";
const VIDEO_SLIDER_ID: u8 = 1;
const REFRESH_BUTTON_ID: u8 = 2;
const BRIGHTNESS_SLIDER_ID: u8 = 3;
const ROTATION_SLIDER_ID: u8 = 4;
const SPEED_SLIDER_ID: u8 = 5;
const FRY_SLIDER_ID: u8 = 6;
const SET_BUTTON_ID: u8 = 7;

const CURL_HEADERS: &str = concat!(
    " -H 'User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0'",
    " -H 'Accept: */*'",
    " -H 'Accept-Language: en-US,en;q=0.5'",
    " -H 'Accept-Encoding: gzip, deflate, br, zstd'",
    " -H 'Referer: http://localhost:8081/admin'",
    " -H 'Connection: keep-alive'",
    " -H 'Cookie: next-auth.session-token=eyJhbGciOiJkaXIiLCJlbmMiOiJBMjU2R0NNIn0..tG9sxpYMw83-nwLC.i3neJSZKLBP0FXsPcE72pVyMdWAOztgToZeASqa4V9yDYIrcioIW3upLFJPz8If2C-7JZU-B1PnrgIv0mWKi_0nekvlUHf42X_SonK6qdGtuznTzuXsmLnDQHSvKczv4rz6kp0Tuvxb4E5QYsaEH2h9PXlb03hBLhKKEXj50sBFDbi51uLuMfqfGbphvGFw8OSEHRGoes_YWSbSZXCS5E_IdBEKBDuAlr1AwSCcirNXfjXvvCKxiovXhEXfqgByk-SQHQcKhrtlN3Q8hq58iveK8i9T_9QtvzyaVi5An97mIDVzu979c4wbR-jcvcpD_MFdtqPVYKm4JnkJVw4LJ_93-EmC6fj3J-Xqh0N2hBD676IR7ICz0nnU6KYkP__Rm3e0HUc8XK7jcuwyUqXtBR6TadX7_nCvBOVs6yrNOlGJG9LNzim7cxfHvt4XeZE0ELYMUiRRFrEE3l35h1qp8hZzxyw5V33oJygvINirVnhn3omYL86t49fn4_y0ldiMt9aDNPYF1pPKladowE0S6EYOu2UXzWCc8JmkQO7Ig-jYotqjKmZiNMs1AtdLwCEb8DVR8D9jBXGYJ3BBGz-R9z5R2JpYZmn8J_q-dkPmKKk-N2Z8NvAsAOzBSvlWpuweN3yyZK60nYgpUpxuQlhC2rT7G_jh_IqwyEMVQ4lpASSbXRbDg-C1_TIsgd2dk5acyS-waDEH8NuL7DQ86V0EKClaHuEYKVcjHqV0pUgIzr-W-_-zR4ck9gmculkwakYwPb-2o-9XGlcSbLsxC1fw5wr_HrYbCg-QnyVNNlDmqj4F3N-xIROdmrTUCfn7FVL5cGcWytVnpLNbsvkJexMmaHdix3-pt0hbsnwZH4MtF5qA23Wx-6M09GTicvYm6LL3TVXAo2T-EASHAz_3uGnv4P3lZJCEMbiwN79BvdGgJvXppliYNKxIbDtwFpfjoFDy5KjdRQ1ey_y2_g3-Yv72EmulAcarWwirtcRhJJB_8k8ozJ71J_4aWC3-aToesROh4J6dyWxTWyEVuABIAq3faZnC9M0DF3o828xkVyUF1inJaYb7MNItYjWIjuQXsBpLdJAQZFYP8GLRo0MOdr-a3D0Dfo6zXISHG3GTm7wL9otITs8nhT1qYh49g4bwY2eLgIWesTk-a9B4I0s5gQJ5kdA.4RlaTUGvrrYgaTix25Yfxw; ph_phc_HFcwzERJWM9IpAuyxelJdn24G2ZSbJAVDp2BV2n3SZW_posthog=%7B%22distinct_id%22%3A%22user_14d4e775-e6dd-41cb-8798-1ffe3f77b23f%22%2C%22%24sesid%22%3A%5Bnull%2Cnull%2Cnull%5D%2C%22%24epp%22%3Atrue%2C%22%24initial_person_info%22%3A%7B%22r%22%3A%22http%3A%2F%2Flocalhost%3A3000%2Fhome%22%2C%22u%22%3A%22http%3A%2F%2Flocalhost%3A3000%2Fcourse%2F7cc11edb-d2fa-4f54-a867-709ca98d0852%22%7D%7D'",
    " -H 'Sec-Fetch-Dest: empty'",
    " -H 'Sec-Fetch-Mode: cors'",
    " -H 'Sec-Fetch-Site: same-origin'",
    " -H 'Priority: u=4'",
);

#[derive(Debug, Default, Clone, Deserialize)]
struct PlaybackSnapshot {
    #[serde(default)]
    file: String,
    #[serde(default)]
    speed: f32,
    #[serde(default)]
    fry: i32,
    #[serde(default)]
    rotation: i32,
    #[serde(default)]
    brightness: i32,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    #[serde(default)]
    playback: PlaybackSnapshot,
}

#[derive(Debug, Deserialize)]
struct VideosResponse {
    #[serde(default)]
    videos: Vec<String>,
}

#[derive(Default)]
pub struct VideowallPlugin {
    videos: Vec<String>,
    selected_index: u8,
    playback: PlaybackSnapshot,
    pending_selection: Option<u8>,
    last_error: Option<String>,
    pending_brightness: u8,
    pending_rotation: u8,
    pending_speed: u8,
    pending_fry: u8,
}

impl VideowallPlugin {
    fn initialize_state(&mut self) {
        if let Err(err) = self.refresh_videos() {
            self.last_error = Some(err);
        }

        match self.refresh_status() {
            Ok(_) => {
                self.align_selection_with_playback();
                if self
                    .last_error
                    .as_deref()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                {
                    self.last_error = None;
                }
            }
            Err(err) => {
                self.last_error = Some(err);
            }
        }

        self.sync_pending_from_playback();

        if let Some(idx) = self.pending_selection.take() {
            self.apply_selection_or_queue(idx);
        }
    }

    fn refresh_videos(&mut self) -> Result<(), String> {
        let payload = self.fetch_endpoint("/admin/api/videos")?;

        let mut response: VideosResponse =
            serde_json::from_str(&payload).map_err(|err| format!("videos json error: {err}"))?;

        response.videos.sort();
        self.videos = response.videos;

        if let Some(idx) = self.pending_selection {
            self.apply_selection_or_queue(idx);
        } else {
            self.align_selection_with_playback();
        }

        Ok(())
    }

    fn refresh_status(&mut self) -> Result<(), String> {
        let payload = self.fetch_endpoint("/admin/api/status")?;

        let response: StatusResponse =
            serde_json::from_str(&payload).map_err(|err| format!("status json error: {err}"))?;

        self.playback = response.playback;
        self.align_selection_with_playback();
        self.sync_pending_from_playback();

        Ok(())
    }

    fn fetch_endpoint(&self, endpoint: &str) -> Result<String, String> {
        let command = format!("curl 'http://localhost:8081{}'{}", endpoint, CURL_HEADERS);
        let output = bpf::system(&command);

        if output.trim().is_empty() {
            Err(format!("command produced empty response: {endpoint}"))
        } else {
            Ok(output)
        }
    }

    fn process_events(&mut self, input: &TickInput) {
        for message in &input.events.events {
            self.dispatch_event(message, input.id);
        }
    }

    fn dispatch_event(&mut self, event: &ControlEventMessage, plugin_id: u8) {
        match event.body() {
            ControlEvent::MiscEvent { descriptor, value } => {
                self.handle_misc_event(descriptor, value)
            }
            ControlEvent::PluginUi(ui_event, pid) if pid == plugin_id => {
                self.handle_ui_event(&ui_event)
            }
            _ => {}
        }
    }

    fn handle_ui_event(&mut self, event: &PluginUiEvent) {
        match event {
            PluginUiEvent::Slider { id, value } | PluginUiEvent::HFader { id, value }
                if *id == VIDEO_SLIDER_ID =>
            {
                self.apply_selection_or_queue(*value);
            }
            PluginUiEvent::Slider { id, value } | PluginUiEvent::HFader { id, value }
                if *id == BRIGHTNESS_SLIDER_ID =>
            {
                self.pending_brightness = *value;
            }
            PluginUiEvent::Slider { id, value } | PluginUiEvent::HFader { id, value }
                if *id == ROTATION_SLIDER_ID =>
            {
                self.pending_rotation = *value;
            }
            PluginUiEvent::Slider { id, value } | PluginUiEvent::HFader { id, value }
                if *id == SPEED_SLIDER_ID =>
            {
                self.pending_speed = *value;
            }
            PluginUiEvent::Slider { id, value } | PluginUiEvent::HFader { id, value }
                if *id == FRY_SLIDER_ID =>
            {
                self.pending_fry = *value;
            }
            PluginUiEvent::Button { id } if *id == REFRESH_BUTTON_ID => {
                if let Err(err) = self.refresh_videos() {
                    self.last_error = Some(err);
                } else if let Err(err) = self.refresh_status() {
                    self.last_error = Some(err);
                } else {
                    self.last_error = None;
                }
            }
            PluginUiEvent::Button { id } if *id == SET_BUTTON_ID => {
                self.apply_brightness(self.pending_brightness);
                self.apply_rotation(self.pending_rotation);
                self.apply_speed(self.pending_speed);
                self.apply_fry(self.pending_fry);
                self.last_error = None;
            }
            _ => {}
        }
    }

    fn handle_misc_event(&mut self, descriptor: u8, value: u8) {
        match descriptor {
            SET_BRIGHTNESS => self.apply_brightness(value),
            SET_ROTATION => self.apply_rotation(value),
            SET_SPEED => self.apply_speed(value),
            SET_FRY => self.apply_fry(value),
            SET_VIDEO_INDEX => self.apply_selection_or_queue(value),
            REQUEST_STATUS_REFRESH => {
                if let Err(err) = self.refresh_status() {
                    self.last_error = Some(err);
                } else {
                    self.last_error = None;
                }
            }
            _ => {}
        }
    }

    fn apply_selection_or_queue(&mut self, index: u8) {
        if self.videos.get(index as usize).is_some() {
            self.pending_selection = None;
            self.send_video_by_index(index as usize);
        } else {
            self.pending_selection = Some(index);
        }
    }

    fn apply_brightness(&mut self, value: u8) {
        bpf::send_udp(UDP_ENDPOINT, &[130, value]);
        self.playback.brightness = value as i32;
        self.pending_brightness = value;
    }

    fn apply_rotation(&mut self, value: u8) {
        let rotation = (value as u16).map_range(0..127, 0..360);
        let rotation_bytes = rotation.to_le_bytes();

        let mut payload = Vec::with_capacity(1 + rotation_bytes.len());
        payload.push(120);
        payload.extend_from_slice(&rotation_bytes);

        bpf::send_udp(UDP_ENDPOINT, &payload);
        self.playback.rotation = rotation as i32;
        self.pending_rotation = value;
    }

    fn apply_speed(&mut self, value: u8) {
        let speed = 0.25 + (value as f32 / 127.0) * (2.0 - 0.25);
        let mut payload = Vec::with_capacity(1 + std::mem::size_of::<f32>());
        payload.push(200);
        payload.extend_from_slice(&speed.to_le_bytes());

        bpf::send_udp(UDP_ENDPOINT, &payload);
        self.playback.speed = speed;
        self.pending_speed = value;
    }

    fn apply_fry(&mut self, value: u8) {
        bpf::send_udp(UDP_ENDPOINT, &[110, value]);
        self.playback.fry = (value as i32) * 10;
        self.pending_fry = value;
    }

    fn send_video_by_index(&mut self, index: usize) {
        if let Some(file) = self.videos.get(index) {
            let mut payload = Vec::with_capacity(1 + file.len());
            payload.push(100);
            payload.extend_from_slice(file.as_bytes());

            bpf::send_udp(UDP_ENDPOINT, &payload);
            self.selected_index = index as u8;
            self.playback.file = file.clone();
        } else {
            self.pending_selection = Some(index as u8);
        }
    }

    fn align_selection_with_playback(&mut self) {
        if self.videos.is_empty() {
            self.selected_index = 0;
            return;
        }

        if let Some(index) = self
            .videos
            .iter()
            .position(|candidate| candidate == &self.playback.file)
        {
            self.selected_index = index as u8;
        } else if self.selected_index as usize >= self.videos.len() {
            self.selected_index = 0;
        }
    }

    fn sync_pending_from_playback(&mut self) {
        self.pending_brightness = self.playback.brightness.clamp(0, 255) as u8;
        self.pending_rotation = degrees_to_midi(self.playback.rotation);
        self.pending_speed = speed_to_midi(self.playback.speed);
        self.pending_fry = fry_to_midi(self.playback.fry);
    }

    fn render_ui(&self) {
        bpf::ui::begin();
        bpf::ui::begin_frame_styled(10, "Videowall", 8, 8, 4, 4);

        if let Some(err) = &self.last_error {
            bpf::ui::label("Status: error");
            bpf::ui::label(err);
        } else {
            let current_video = self
                .videos
                .get(self.selected_index as usize)
                .cloned()
                .or_else(|| {
                    if self.playback.file.is_empty() {
                        None
                    } else {
                        Some(self.playback.file.clone())
                    }
                })
                .unwrap_or_else(|| "—".to_string());

            bpf::ui::label(&format!("Current video: {}", current_video));
            bpf::ui::label(&format!("Brightness: {}", self.pending_brightness));
            bpf::ui::hfader(
                "Brightness",
                BRIGHTNESS_SLIDER_ID,
                0,
                255,
                self.pending_brightness,
            );

            let rotation_degrees = midi_to_degrees(self.pending_rotation);
            bpf::ui::label(&format!("Rotation: {}°", rotation_degrees));
            bpf::ui::hfader(
                "Rotation",
                ROTATION_SLIDER_ID,
                0,
                127,
                self.pending_rotation,
            );

            let speed_value = midi_to_speed(self.pending_speed);
            bpf::ui::label(&format!("Speed: {:.2}x", speed_value));
            bpf::ui::hfader("Speed", SPEED_SLIDER_ID, 0, 127, self.pending_speed);

            let fry_value = midi_to_fry(self.pending_fry);
            bpf::ui::label(&format!("Fry: {}", fry_value));
            bpf::ui::hfader("Fry", FRY_SLIDER_ID, 0, 127, self.pending_fry);
        }

        if !self.videos.is_empty() {
            let max_index = (self.videos.len().saturating_sub(1)).min(u8::MAX as usize) as u8;
            let selected = self.selected_index.min(max_index);
            bpf::ui::hfader("Video", VIDEO_SLIDER_ID, 0, max_index, selected);
        } else {
            bpf::ui::label("No videos discovered.");
        }

        bpf::ui::button("Refresh", REFRESH_BUTTON_ID);
        bpf::ui::button("Set", SET_BUTTON_ID);
        bpf::ui::end_frame();
    }
}

impl Plugin for VideowallPlugin {
    fn initialize(&mut self, input: TickInput) {
        self.process_events(&input);
        self.initialize_state();
    }

    fn run(&mut self, input: TickInput) {
        self.process_events(&input);
        self.render_ui();
    }
}

#[no_mangle]
extern "C" fn main() {
    bpf::hook_plugin(Box::new(VideowallPlugin::default()));
}

fn midi_to_degrees(value: u8) -> u16 {
    (value as u16).map_range(0..127, 0..360)
}

fn degrees_to_midi(degrees: i32) -> u8 {
    let clamped = degrees.clamp(0, 359) as u16;
    clamped.map_range(0..360, 0..127) as u8
}

fn midi_to_speed(value: u8) -> f32 {
    0.25 + (value as f32 / 127.0) * (2.0 - 0.25)
}

fn speed_to_midi(speed: f32) -> u8 {
    let clamped = speed.clamp(0.25, 2.0);
    let normalized = ((clamped - 0.25) / (2.0 - 0.25)) * 127.0;
    normalized.round().clamp(0.0, 127.0) as u8
}

fn midi_to_fry(value: u8) -> u16 {
    value as u16 * 10
}

fn fry_to_midi(fry: i32) -> u8 {
    (fry / 10).clamp(0, 127) as u8
}
